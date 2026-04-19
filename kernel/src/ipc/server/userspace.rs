use core::future::Future;
use core::num::NonZero;
use core::pin::Pin;
use core::task::{Context, Poll, Waker};
use slab::Slab;
use kernel_api::{channel, dbg};
use kernel_api::mapping::{Config, Mmap, Ty};
use kernel_api::memory::VirtualAddress;
use kernel_api::ptr::{local_slice_from_raw_parts, LocalUser};
use kernel_api::sync::Spinlock;
use crate::ipc::ctor::CtorArgs;
use crate::ipc::Error;
use crate::ipc::server::ReturnHandle;
use crate::ipc::serde::{Deserialized, MethodResult, Serializer};
use kernel_api::address_space::MappingKey;
use kernel_api::channel::Receiver;
use kernel_api::syscall::server::ServerId;

#[derive(Debug, Copy, Clone)]
#[repr(transparent)]
struct PacketKey(usize);

#[derive(Debug)]
pub struct UserspaceServer {
	pending_queue: Receiver<PacketKey>,
	in_flight: Spinlock<Slab<(RawPacket, Waker)>>,
}

impl UserspaceServer {
	pub fn new_current_thread() -> Self {
		Self {
			pending_queue: channel::unbounded().1,
			in_flight: Spinlock::new(Slab::new()),
		}
	}

	pub async fn get_packet(&self) -> Packet {
		let packet_key = self.pending_queue.pop().await;

		let mut guard = self.in_flight.lock();
		let (raw, _) = guard.get_mut(packet_key.0).expect("invalid key in pending queue");
		let RawPacket::Pending(args) = core::mem::replace(raw, RawPacket::None) else { panic!("unexpected packet in bagging area"); };

		let guard = percpu_v2!(current_thread).read();
		let address_space = &guard
				.as_ref()
				.expect("cannot syscall from idle")
				.address_space;

		match args {
			PacketArgs::Ctor { endpoint, protocols } => {
				let buffer_size = protocols.len() * size_of::<u128>() + endpoint.len();
				let buffer_size = NonZero::new(buffer_size).expect("must have non-zero buffer size");
				let buffer_size = buffer_size.div_ceil(NonZero::new(4096).unwrap());

				let (mapping_key, mut buffer) = Config::new(buffer_size, Ty::USER_PACKET_BUFFER)
						.protection(true, false, true)
						.map_in::<Mmap>("[packet buffer]".into(), address_space)
						.unwrap();

				let buffer_start = buffer.as_mut_ptr();

				debug!("{address_space:?}");

				assert!(buffer_start.is_aligned_to(align_of::<u128>()));

				let protocols_ptr = buffer_start.cast();
				let endpoint_ptr = unsafe { buffer_start.byte_offset((protocols.len() * size_of::<u128>()) as isize) };

				unsafe {
					protocols_ptr.copy_from_other_address_space(protocols.as_ptr(), protocols.len()).unwrap();
					endpoint_ptr.copy_from_other_address_space(endpoint.as_ptr(), endpoint.len()).unwrap();
				}

				*raw = RawPacket::Processing {
					// serializer isn't actually used for `ctor` so just pass an invalid one that makes no allocation
					serializer: Serializer::new(ServerId::INVALID, Box::from([])),
					buffer: Some(mapping_key),
				};

				dbg!(Packet {
					uid: (crate::ipc::abi_v1::NEW as u128) << 96 | crate::ipc::abi_v1::ABI_V1,
					packet: packet_key,
					arg0: endpoint_ptr.addr().addr,
					arg1: endpoint.len(),
					arg2: protocols_ptr.addr().addr,
					arg3: protocols.len(),
					arg4: 0,
				})
			},
			PacketArgs::Dtor { handle } => {
				*raw = RawPacket::Processing {
					// serializer isn't actually used for `dtor` so just pass an invalid one that makes no allocation
					serializer: Serializer::new(ServerId::INVALID, Box::from([])),
					buffer: None,
				};

				dbg!(Packet {
					uid: (crate::ipc::abi_v1::DESTROY as u128) << 96 | crate::ipc::abi_v1::ABI_V1,
					packet: packet_key,
					arg0: handle as usize,
					arg1: 0,
					arg2: 0,
					arg3: 0,
					arg4: 0,
				})
			},
			PacketArgs::Other {
				uid,
				args,
			} => {
				let (mapping_key, mapping_addr) = {
					let buffer_data = args.buffer();
					let buffer_size = buffer_data.len().div_ceil(4096);

					match NonZero::new(buffer_size) {
						None => (None, VirtualAddress::new(0)),
						Some(buffer_size) => {
							let (mapping_key, mut buffer) = Config::new(buffer_size, Ty::USER_PACKET_BUFFER)
									.protection(true, false, true)
									.map_in::<Mmap>("[packet buffer]".into(), address_space)
									.unwrap();

							unsafe {
								buffer.as_mut_ptr().copy_from_other_address_space(buffer_data.as_ptr().cast(), buffer_data.len()).unwrap();
							}

							(Some(mapping_key), buffer.as_ptr().addr())
						}
					}
				};

				let (args, serializer) = args.into_args_with_buffer(mapping_addr);

				*raw = RawPacket::Processing {
					serializer,
					buffer: mapping_key,
				};

				dbg!(Packet {
					uid,
					packet: packet_key,
					arg0: args[0],
					arg1: args[1],
					arg2: args[2],
					arg3: args[3],
					arg4: args[4],
				})
			},
		}
	}

	pub fn reply_packet(&self, response: Response) -> Result<(), Error> {
		debug!("received response {response:?}");
		let mut guard = self.in_flight.lock();
		let (raw, waker) = &mut guard.get_mut(response.packet.0).expect("invalid key in pending queue");

		let result = if response.error {
			match response.result {
				ReturnVal::Value(val) => Err(Error::from(val)),
				_ => return Err(Error::InvalidArg),
			}
		} else {
			Ok(response.result.into_method_result()?)
		};

		let RawPacket::Processing { mut serializer, buffer } = core::mem::replace(raw, RawPacket::None) else { panic!("unexpected packet in bagging area"); };

		if let Some(buffer) = buffer {
			let guard = percpu_v2!(current_thread).read();
			let address_space = &guard
					.as_ref()
					.expect("cannot syscall from idle")
					.address_space;
			let buffer = address_space.get(buffer)
					.expect("invalid buffer key");

			if result.is_ok() {
				let buffer_ptr = buffer.as_ptr().try_into()
						.expect("mapping for current address space must be in current address space");
				let ptr = unsafe { local_slice_from_raw_parts(buffer_ptr, buffer.byte_len()) };
				ptr.read_to_buffer(serializer.buffer_uninit().get_mut())?;
			};
			buffer.remove();
		}
		
		*raw = RawPacket::Done {
			result,
			serializer,
		};

		debug!("userspace reply with {raw:#x?}");
		
		waker.wake_by_ref();
		
		Ok(())
	}

	fn push_packet(&self, packet: RawPacket, waker: Waker) -> PacketKey {
		debug!("push packet: {packet:#x?}");
		
		let key = self.in_flight.lock().insert((packet, waker));
		self.pending_queue.push(PacketKey(key));
		PacketKey(key)
	}

	fn pop_done_packet(&self, key: PacketKey, new_waker: Waker) -> Option<RawPacket> {
		let mut guard = self.in_flight.lock();
		if let Some(packet) = guard.get_mut(key.0) {
			if matches!(packet.0, RawPacket::Done { .. }) {
				Some(guard.remove(key.0).0)
			} else {
				packet.1 = new_waker;
				None
			}
		} else { None }
	}

	pub async fn ctor(&self, endpoint: &str, args: CtorArgs<'_>) -> Result<ReturnHandle, Error> {
		let result = UserspaceFuture::Unsubmitted(self, RawPacket::Pending(
			PacketArgs::Ctor {
				endpoint: Box::from(endpoint),
				protocols: Box::from(args.uids()),
			}),
		).await.0;

		result.and_then(|v| {
			match v {
				MethodResult::Value(_) => Err(Error::InvalidReturn),
				MethodResult::TransferHandle(handle) => Ok(ReturnHandle::Transfer(handle)),
				MethodResult::SelfHandle(id, protos) => Ok(ReturnHandle::New(id, protos, endpoint.to_owned().into())),
				MethodResult::SelfDefaultHandle(id) => Ok(ReturnHandle::NewDefault(id)),
			}
		})
	}

	pub async fn destroy(&self, handle: isize) -> Result<(), Error> {
		let _ = UserspaceFuture::Unsubmitted(self, RawPacket::Pending(
			PacketArgs::Dtor {
				handle
			}),
		).await.0?;
		
		Ok(())
	}

	pub fn dispatch(
		&self,
		protocol: u128,
		method: u32,
		args: Deserialized,
	) -> impl Future<Output = (Result<MethodResult, Error>, Serializer)> + '_ {
		let uid = protocol | (method as u128) << 96;
		
		UserspaceFuture::Unsubmitted(self, RawPacket::Pending(
			PacketArgs::Other {
				uid,
				args,
			})
		)
	}
}

enum UserspaceFuture<'a> {
	Unsubmitted(&'a UserspaceServer, RawPacket),
	Submitted(&'a UserspaceServer, PacketKey),
}

impl<'a> Future for UserspaceFuture<'a> {
	type Output = (Result<MethodResult, Error>, Serializer);

	fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
		let this = self.get_mut();
		match this {
			Self::Unsubmitted(server, raw) => {
				debug!("submit userspace packet {raw:#?}");
				let raw = unsafe { core::ptr::read(raw) };
				let server = unsafe { core::ptr::read(server) };
				
				let key = server.push_packet(raw, cx.waker().clone());
				unsafe { core::ptr::write(this, Self::Submitted(server, key)) };
				debug!("submitted packet to userspace - returning pending");
				Poll::Pending
			}
			Self::Submitted(server, key) => {
				match server.pop_done_packet(*key, cx.waker().clone()) {
					None => {
						debug!("userspace packet still pending");
						Poll::Pending
					},
					Some(RawPacket::Done { result, serializer }) => dbg!(Poll::Ready((result, serializer))),
					_ => unreachable!("unexpected PacketState in bagging area")
				}
			}
		}
	}
}

#[derive(Debug, Copy, Clone)]
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

#[derive(Debug, Copy, Clone)]
#[repr(C)]
pub struct Response {
	result: ReturnVal,
	packet: PacketKey,
	error: bool,
}

#[derive(Debug, Copy, Clone)]
#[repr(C)]
#[allow(dead_code)]
enum ReturnVal {
	SelfHandle(isize, *const u128, usize),
	SelfDefaultHandle(isize),
	TransferHandle(isize),
	Value(u128),
}

impl ReturnVal {
	fn into_method_result(self) -> Result<MethodResult, Error> {
		Ok(match self {
			ReturnVal::SelfHandle(num, ptr, len) => {
				let ptr = {
					let ptr = unsafe { LocalUser::<*const u128>::new(ptr.addr()) };
					unsafe { local_slice_from_raw_parts(ptr, len) }
				};
				let protos = ptr.read_to_box()?;
				MethodResult::SelfHandle(num, protos)
			},
			ReturnVal::SelfDefaultHandle(handle) => MethodResult::SelfDefaultHandle(handle),
			ReturnVal::TransferHandle(id) => MethodResult::TransferHandle({
				let id = id.try_into().map_err(|_| Error::InvalidHandle)?;
				percpu_v2!(current_thread)
						.read()
						.as_ref()
						.expect("cannot syscall from idle")
						.handles
						.pop(id)?
			}),
			ReturnVal::Value(val) => MethodResult::Value(val),
		})
	}
}

#[derive(Debug)]
pub enum PacketArgs {
	Ctor {
		endpoint: Box<str>,
		protocols: Box<[u128]>,
	},
	Dtor {
		handle: isize,
	},
	Other {
		uid: u128,
		args: Deserialized,
	},
}

#[derive(Debug)]
enum RawPacket {
	Pending(PacketArgs),
	Processing { serializer: Serializer, buffer: Option<MappingKey> },
	Done { result: Result<MethodResult, Error>, serializer: Serializer },
	None,
}
