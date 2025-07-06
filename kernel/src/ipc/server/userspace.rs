use core::cmp::min;
use core::iter::zip;
use core::mem::{ManuallyDrop, MaybeUninit};
use core::num::NonZero;
use core::ops::Range;
use crate::prelude::*;
use core::sync::atomic::{AtomicPtr, Ordering};
use crossbeam_queue::SegQueue;
use slab::Slab;
use kernel_api::memory::mapping;
use kernel_api::memory::mapping::{new_mapping_in, Protection};
use kernel_api::ptr::{slice_from_raw_parts, slice_from_raw_parts_mut, User};
use kernel_api::sync::Spinlock;
use crate::ipc::ctor::{CtorArgs, CtorContext, ProtocolVisitor};
use crate::ipc::{dispatch, Error};
use crate::ipc::protocol::{DispatchTable, meta};
use crate::ipc::server::{ReturnHandle, Server};
use crate::{non_zero, threading};
use crate::ipc::dispatch::{Arg, Return, DeserializedArgs};
use crate::ipc::handle::ServerId;
use crate::ipc::protocol::meta::Meta;
use crate::memory::r#virtual::{AddressSpaceInner, MappingKey};
use crate::threading::{ThreadId, WakeReason, WakeTrigger};

#[derive(Debug)]
#[repr(transparent)]
struct PacketKey(usize);

#[derive(Debug)]
pub struct UserspaceServer {
	pending_queue: SegQueue<PacketKey>,
	waker: AtomicPtr<u8>, // this is actually an Option<WakeTrigger>
	in_flight: Spinlock<Slab<RawPacket>>,
}

impl UserspaceServer {
	pub fn new_current_thread() -> Self {
		Self {
			pending_queue: SegQueue::new(),
			waker: AtomicPtr::new(core::ptr::null_mut()),
			in_flight: Spinlock::new(Slab::new()),
		}
	}

	pub fn get_packet_blocking(&self) -> Packet {
		let mut waker = core::ptr::null_mut();
		let packet_key = loop {
			if let Ok(val) = threading::maybe_park(|trigger| {
				waker = unsafe { core::mem::transmute(trigger) };
				self.waker.store(waker, Ordering::Relaxed);
				self.pending_queue.pop()
			}) { break val; }
		};
		let _ = self.waker.compare_exchange(waker, core::ptr::null_mut(), Ordering::Relaxed, Ordering::Relaxed);

		let mut guard = self.in_flight.lock();
		let raw = guard.get_mut(packet_key.0).expect("invalid key in pending queue");
		let PacketState::Pending(args) = core::mem::replace(&mut raw.state, PacketState::None) else { panic!("unexpected packet in bagging area"); };

		match args {
			PacketArgs::Ctor { endpoint, protocols, args } => {
				let buffer_size = protocols.len() * size_of::<u128>() + endpoint.len() + args.len();
				let buffer_size = NonZero::new(buffer_size).expect("must have non-zero buffer size");
				let buffer_size = buffer_size.div_ceil(NonZero::new(4096).unwrap());

				let guard = percpu_v2!(current_thread).read();
				let address_space = guard.as_ref().expect("cannot generate packet from idle thread")
				                                              .tcb_ref().address_space;

				let config = mapping::Config::new_in(
					buffer_size,
					AddressSpaceInner::to_api(address_space)
				).protection(Protection::RWXU);

				let buffer = new_mapping_in(config, u16::MAX).unwrap();

				let buffer_start = User::<*mut u8>::new_in(
					buffer.virtual_valid_start().as_ptr(),
					AddressSpaceInner::to_api(address_space),
				);

				let mapping_key = address_space.add_mapping("[packet buffer]", buffer);
				trace!("{address_space:?}");

				assert!(buffer_start.is_aligned_to(align_of::<u128>()));

				let protocols_ptr = buffer_start.cast();
				let endpoint_ptr = unsafe { buffer_start.byte_offset((protocols.len() * size_of::<u128>()) as isize) };
				let args_ptr = unsafe { endpoint_ptr.byte_offset(endpoint.len() as isize) };

				protocols_ptr.copy_from_nonoverlapping(protocols.as_ptr(), protocols.len()).unwrap();
				endpoint_ptr.copy_from_nonoverlapping(endpoint.as_ptr(), endpoint.len()).unwrap();
				args_ptr.copy_from_nonoverlapping(args.as_ptr(), args.len()).unwrap();

				raw.state = PacketState::Processing { return_buffer: None };
				raw.buffer = Some(mapping_key);

				Packet {
					uid: 1u128 << 96,
					packet: packet_key,
					arg0: endpoint_ptr.addr(),
					arg1: endpoint.len(),
					arg2: protocols_ptr.addr(),
					arg3: protocols.len(),
					arg4: args_ptr.addr(),
				}
			},
			PacketArgs::Dtor { handle } => {
				raw.state = PacketState::Processing { return_buffer: None };

				Packet {
					uid: 3u128 << 96,
					packet: packet_key,
					arg0: handle as usize,
					arg1: 0,
					arg2: 0,
					arg3: 0,
					arg4: 0,
				}
			},
			PacketArgs::Other {
				uid,
				args,
			} => {
				let buffer_size = args.buffer.len() + args.return_size.unwrap_or(0);
				let (buffer_start, mapping_key) = if let Some(buffer_size) = NonZero::new(buffer_size) {
					let buffer_size = buffer_size.div_ceil(NonZero::new(4096).unwrap());

					let guard = percpu_v2!(current_thread).read();
					let address_space = guard.as_ref().expect("cannot generate packet from idle thread")
					                         .tcb_ref().address_space;

					let config = mapping::Config::new_in(
						buffer_size,
						AddressSpaceInner::to_api(address_space)
					).protection(Protection::RWXU);

					let buffer = new_mapping_in(config, u16::MAX).unwrap();

					let buffer_start = User::<*mut u8>::new_in(
						buffer.virtual_valid_start().as_ptr(),
						AddressSpaceInner::to_api(address_space),
					);

					let mapping_key = address_space.add_mapping("[packet buffer]", buffer);
					trace!("{address_space:?}");

					buffer_start.copy_from_nonoverlapping(args.buffer.as_ptr(), args.buffer.len()).unwrap();

					(buffer_start, Some(mapping_key))
				} else { (User::null_mut(), None) };

				let to_arg = |arg| match arg {
					Arg::Primitive(val) => val,
					Arg::BufferOffset(off) => unsafe { buffer_start.byte_offset(off as isize) }.addr(),
					Arg::NewBuffer => unsafe { buffer_start.byte_offset(args.buffer.len() as isize) }.addr(),
					Arg::Handle(handle) => {
						let handle = percpu_v2!(current_thread).read().as_ref()
						                          .expect("must be running on a thread to syscall")
						                          .tcb_ref().handles.push(handle).unwrap();
						handle as usize
					}
				};

				raw.state = PacketState::Processing {
					return_buffer: if let Some(size) = args.return_size {
						Some(slice_from_raw_parts(
							unsafe { buffer_start.cast_const().byte_offset(args.buffer.len() as isize) },
							size
						))
					} else { None },
				};
				raw.buffer = mapping_key;
				
				// Wrap this in a ManuallyDrop so we can pull them out the array without cloning
				let mut args = args.args.map(ManuallyDrop::new);

				Packet {
					uid,
					packet: packet_key,
					arg0: to_arg(unsafe { ManuallyDrop::take(&mut args[0]) }),
					arg1: to_arg(unsafe { ManuallyDrop::take(&mut args[1]) }),
					arg2: to_arg(unsafe { ManuallyDrop::take(&mut args[2]) }),
					arg3: to_arg(unsafe { ManuallyDrop::take(&mut args[3]) }),
					arg4: to_arg(unsafe { ManuallyDrop::take(&mut args[4]) }),
				}
			},
		}
	}

	pub fn reply_packet(&self, response: Response) -> Result<(), Error> {
		let mut guard = self.in_flight.lock();
		let raw = guard.get_mut(response.packet.0).expect("invalid key in pending queue");

		let result = if response.error { Err(response.result) } else { Ok(response.result) };
		let result = match result {
			Ok(ReturnVal::SelfHandle(id, ptr, len)) => {
				let protocols = {
					let protocol_ptr = User::<*const u128>::new_in(
						ptr as _,
						AddressSpaceInner::to_api(
							percpu_v2!(current_thread).read().as_ref().expect("can only syscall from thread")
							                          .tcb_ref().address_space
						),
					);
					slice_from_raw_parts(protocol_ptr, len)
				};
				debug!("protocols: {protocols:#p}");
				let protocols = unsafe { protocols.read_to_buffer() }?;
				debug!("protocols: {protocols:#x?}");
				
				Ok(Return::NewHandle(id, protocols))
			},
			Ok(ReturnVal::SelfDefaultHandle(id)) => Ok(Return::NewDefaultHandle(id)),
			Ok(ReturnVal::TransferHandle(id)) => {
				let handle = percpu_v2!(current_thread).read().as_ref()
				                                       .expect("can only syscall from thread")
				                                       .tcb_ref().handles.pop(id.try_into().map_err(|_| Error::InvalidHandle)?)?;
				Ok(Return::Handle(handle))
			},
			Ok(ReturnVal::Value(val)) => Ok(Return::Value(val)),
			Err(ReturnVal::Value(val)) => Err(Error::from(val)),
			Err(_) => return Err(Error::InvalidArg),
		};

		let PacketState::Processing { return_buffer } = core::mem::replace(&mut raw.state, PacketState::None) else { panic!("unexpected packet in bagging area"); };
		
		let result = if let Ok(result) = &result && let Some(buffer) = return_buffer {
			let Return::Value(result) = result else {
				debug!("userspace returned non-primitive return for memory return");
				return Err(Error::InvalidArg);
			};
			let len = min(buffer.len(), *result as usize);
			let buffer = slice_from_raw_parts(buffer.as_ptr(), len);
			Ok(Return::Boxed(unsafe { buffer.read_to_buffer().expect("invalid buffer") }))
		} else { result };
		
		if let Some(buffer) = raw.buffer {
			warn!("ignoring dealloc of buffer {buffer:?}");
		}
		
		raw.state = PacketState::Done {
			result,
		};
		
		raw.waker.wake(WakeReason::Custom(non_zero!(2)));
		
		Ok(())
	}

	fn push_packet(&self, packet: RawPacket) -> PacketKey {
		debug!("push packet: {packet:#x?}");
		
		let key = self.in_flight.lock().insert(packet);
		self.pending_queue.push(PacketKey(key));
		let waker = self.waker.load(Ordering::Relaxed);
		if !waker.is_null() {
			debug!("wake waiting userspace server");
			let waker = unsafe { core::mem::transmute::<_, WakeTrigger>(waker) };
			waker.wake(WakeReason::Custom(non_zero!(1)));
		}
		PacketKey(key)
	}

	fn pop_packet(&self, key: PacketKey) -> Option<RawPacket> {
		self.in_flight.lock().try_remove(key.0)
	}

	pub fn ctor(&self, endpoint: &str, args: CtorArgs) -> Result<ReturnHandle, Error> {
		let mut key = PacketKey(usize::MAX);
		threading::maybe_park(|waker| {
			key = self.push_packet(RawPacket {
				state: PacketState::Pending(PacketArgs::Ctor {
					endpoint: Box::from(endpoint),
					protocols: Box::from(args.uids()),
					args: Box::from([]),
				}),
				waker,
				buffer: None,
			});
			None::<()>
		}).expect_err("always parks");

		let packet = self.pop_packet(key).expect("invalid packet key");
		let PacketState::Done { result, .. } = packet.state else { panic!("unexpected state in bagging area"); };
		
		result.and_then(|v| {
			match v {
				Return::Boxed(_) | Return::Value(_) => Err(Error::InvalidArg),
				Return::Handle(handle) => Ok(ReturnHandle::Transfer(handle)),
				Return::NewHandle(id, protos) => Ok(ReturnHandle::New(id, protos)),
				Return::NewDefaultHandle(id) => Ok(ReturnHandle::NewDefault(id)),
			}
		})
	}

	pub fn destroy(&self, handle: isize) -> Result<(), Error> {
		threading::maybe_park(|waker| {
			self.push_packet(RawPacket {
				state: PacketState::Pending(PacketArgs::Dtor {
					handle
				}),
				waker,
				buffer: None,
			});
			None::<()>
		}).expect_err("always parks");
		todo!()
	}

	pub fn dispatch(
		&self,
		protocol: u128,
		method: u32,
		args: DeserializedArgs,
	) -> Result<Return, Error> {
		let uid = protocol | (method as u128) << 96;
		let mut key = PacketKey(usize::MAX);
		let _ = threading::maybe_park(|waker| {
			key = self.push_packet(RawPacket {
				state: PacketState::Pending(PacketArgs::Other {
					uid,
					args,
				}),
				waker,
				buffer: None,
			});
			None::<()>
		}).expect_err("always parks");

		let packet = self.pop_packet(key).expect("invalid packet key");
		debug!("packet: {packet:#x?}");

		let PacketState::Done { result } = packet.state else { panic!("unexpected state in bagging area"); };

		result
	}
}

#[derive(Default)]
pub struct CtorCtx;

impl CtorContext for CtorCtx {
	fn visitors(&self) -> &'static ProtocolVisitor<Self> { const { &ProtocolVisitor::new() } }
}

#[derive(Debug)]
#[repr(C)]
pub struct Packet {
	uid: u128,
	packet: PacketKey,
	arg0: usize,
	arg1: usize,
	arg2: usize,
	arg3: usize,
	arg4: usize,
}

#[derive(Debug)]
#[repr(C)]
pub struct Response {
	result: ReturnVal,
	packet: PacketKey,
	error: bool,
}

#[derive(Debug)]
#[repr(C)]
enum ReturnVal {
	SelfHandle(isize, *const u128, usize),
	SelfDefaultHandle(isize),
	TransferHandle(isize),
	Value(u128),
}

#[derive(Debug)]
pub enum PacketArgs {
	Ctor {
		endpoint: Box<str>,
		protocols: Box<[u128]>,
		args: Box<[u8]>,
	},
	Dtor {
		handle: isize,
	},
	Other {
		uid: u128,
		args: DeserializedArgs,
	},
}

#[derive(Debug)]
struct RawPacket {
	state: PacketState,
	waker: WakeTrigger,
	buffer: Option<MappingKey>,
}

#[derive(Debug)]
enum PacketState {
	Pending(PacketArgs),
	Processing { return_buffer: Option<User<*const [u8]>> },
	Done { result: Result<Return, Error> },
	None,
}
