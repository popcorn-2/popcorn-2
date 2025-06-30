use crate::prelude::*;
use hashbrown::HashMap;
use kernel_api::ptr::User;
use crate::ipc::Error;

pub struct DispatchTable {
	map: HashMap<u128, fn(*const (), usize, usize, usize, usize, usize) -> Result<u128, Error>>,
}

impl DispatchTable {
	pub fn new() -> Self { Self { map: HashMap::new() } }
	pub fn add_vtable(mut self, vtable: HashMap<u128, fn(*const (), usize, usize, usize, usize, usize) -> Result<u128, Error>>) -> Self {
		self.map.extend(vtable);
		self
	}

	pub fn dispatch(
		&self,
		protocol: u128,
		method: u32,
		f_self: *const (),
		arg0: usize,
		arg1: usize,
		arg2: usize,
		arg3: usize,
		arg4: usize
	) -> Result<u128, Error> {
		let uid = protocol | (method as u128) << 96;
		let f = self.map.get(&uid).ok_or(Error::UnsupportedProtocol)?;
		f(f_self, arg0, arg1, arg2, arg3, arg4)
	}
	
	pub fn ctor_deserialize(&self, protocol: u128) -> Result<fn(buffer: &mut User<*const u8>) -> Result<Box<[u8]>, Error>, Error> {
		Ok(unsafe { core::mem::transmute(self.map.get(&protocol).ok_or(Error::UnsupportedProtocol)?) })
	}
}

pub trait Protocol {
	const UID: u128;
	type Ctor<'a>;
}

pub mod generated {
	use alloc::boxed::Box;
	use core::alloc::Layout;
	use core::marker::PhantomData;
	use kernel_api::ptr::{slice_from_raw_parts, slice_from_raw_parts_mut, User};
	use crate::ipc::Error;
	use crate::memory::r#virtual::AddressSpaceInner;
	use crate::percpu::percpu_v2;
	use hashbrown::HashMap;
	use super::Protocol;
	pub trait CoreIoSeekDispatch {

		fn deserialize_ctor(buffer: &mut User<*const u8>) -> Result<Box<[u8]>, Error> where Self: Sized {
			let ctor_args_layout = Layout::new::<CoreIoSeekCtor>();
			unsafe { *buffer = buffer.byte_offset(buffer.align_offset(ctor_args_layout.align()) as isize); }
			let bytes = slice_from_raw_parts(*buffer, ctor_args_layout.size());
			unsafe { *buffer = buffer.byte_offset(ctor_args_layout.size() as isize); }
			unsafe { bytes.read_to_buffer() }.map_err(|_| Error::InvalidPointer)
		}

	}
	impl<T: CoreIoSeek> CoreIoSeekDispatch for T {
	}
	pub trait CoreIoSeek: CoreIoSeekDispatch {
		fn __vtable() -> HashMap<u128, fn(*const (), usize, usize, usize, usize, usize) -> Result<u128, Error>> where Self: Sized { let mut map = HashMap::new(); map.insert_unique_unchecked(4u128, unsafe { core::mem::transmute(<Self as CoreIoSeekDispatch>::deserialize_ctor as fn(_) -> _) });  map }
	}
	impl Protocol for dyn CoreIoSeek { const UID: u128 = 4; type Ctor<'a> = CoreIoSeekCtor<'a>; }
	#[repr(C)] pub struct CoreIoSeekCtor<'a> {
		_phantom: PhantomData<&'a ()> }
	pub trait CoreFsFileDispatch {

		fn deserialize_ctor(buffer: &mut User<*const u8>) -> Result<Box<[u8]>, Error> where Self: Sized {
			let ctor_args_layout = Layout::new::<CoreFsFileCtor>();
			unsafe { *buffer = buffer.byte_offset(buffer.align_offset(ctor_args_layout.align()) as isize); }
			let bytes = slice_from_raw_parts(*buffer, ctor_args_layout.size());
			unsafe { *buffer = buffer.byte_offset(ctor_args_layout.size() as isize); }
			unsafe { bytes.read_to_buffer() }.map_err(|_| Error::InvalidPointer)
		}

	}
	impl<T: CoreFsFile> CoreFsFileDispatch for T {
	}
	pub trait CoreFsFile: CoreFsFileDispatch {
		fn __vtable() -> HashMap<u128, fn(*const (), usize, usize, usize, usize, usize) -> Result<u128, Error>> where Self: Sized { let mut map = HashMap::new(); map.insert_unique_unchecked(1u128, unsafe { core::mem::transmute(<Self as CoreFsFileDispatch>::deserialize_ctor as fn(_) -> _) });  map }
	}
	impl Protocol for dyn CoreFsFile { const UID: u128 = 1; type Ctor<'a> = CoreFsFileCtor<'a>; }
	#[repr(C)] pub struct CoreFsFileCtor<'a> {
		pub create: usize,
		pub append: bool,
		pub truncate: bool,
		_phantom: PhantomData<&'a ()> }
	pub trait CoreIoReadDispatch {
		fn dispatch_read(&self, a0: usize, a1: usize, a2: usize, a3: usize, a4: usize) -> Result<u128, Error>;

		fn deserialize_ctor(buffer: &mut User<*const u8>) -> Result<Box<[u8]>, Error> where Self: Sized {
			let ctor_args_layout = Layout::new::<CoreIoReadCtor>();
			unsafe { *buffer = buffer.byte_offset(buffer.align_offset(ctor_args_layout.align()) as isize); }
			let bytes = slice_from_raw_parts(*buffer, ctor_args_layout.size());
			unsafe { *buffer = buffer.byte_offset(ctor_args_layout.size() as isize); }
			unsafe { bytes.read_to_buffer() }.map_err(|_| Error::InvalidPointer)
		}

	}
	impl<T: CoreIoRead> CoreIoReadDispatch for T {
		fn dispatch_read(&self, a0: usize, a1: usize, a2: usize, a3: usize, a4: usize) -> Result<u128, Error> {
			let v0 = a0 as isize;
			let mut buf = {
				let ptr = User::<*mut u8>::new_in(
					a1 as _,
					AddressSpaceInner::to_api(
						percpu_v2!(current_thread).read().as_ref().expect("can only syscall from thread")
						                          .tcb_ref().address_space
					),
				);
				slice_from_raw_parts_mut(ptr, a2)
			};
			let v1 = a2;
			let ret = <Self as CoreIoRead>::read(self,
				v0,
				v1,
			)?;
			let ret = buf.write_from_buffer(&ret)?;
			Ok(ret as u128)
		}
	}
	pub trait CoreIoRead: CoreIoReadDispatch {
		fn read(&self, handle: isize, output_size: usize,
		) -> Result<Box<[u8]>, Error>;
		fn __vtable() -> HashMap<u128, fn(*const (), usize, usize, usize, usize, usize) -> Result<u128, Error>> where Self: Sized { let mut map = HashMap::new(); map.insert_unique_unchecked(2u128, unsafe { core::mem::transmute(<Self as CoreIoReadDispatch>::deserialize_ctor as fn(_) -> _) }); map.insert_unique_unchecked(2u128 | (1 as u128) << 96, unsafe { core::mem::transmute(<Self as CoreIoReadDispatch>::dispatch_read as fn(_, _, _, _, _, _) -> _) });
			map }
	}
	impl Protocol for dyn CoreIoRead { const UID: u128 = 2; type Ctor<'a> = CoreIoReadCtor<'a>; }
	#[repr(C)] pub struct CoreIoReadCtor<'a> {
		_phantom: PhantomData<&'a ()> }
	pub trait CoreIoWriteDispatch {
		fn dispatch_write(&self, a0: usize, a1: usize, a2: usize, a3: usize, a4: usize) -> Result<u128, Error>;

		fn deserialize_ctor(buffer: &mut User<*const u8>) -> Result<Box<[u8]>, Error> where Self: Sized {
			let ctor_args_layout = Layout::new::<CoreIoWriteCtor>();
			unsafe { *buffer = buffer.byte_offset(buffer.align_offset(ctor_args_layout.align()) as isize); }
			let bytes = slice_from_raw_parts(*buffer, ctor_args_layout.size());
			unsafe { *buffer = buffer.byte_offset(ctor_args_layout.size() as isize); }
			unsafe { bytes.read_to_buffer() }.map_err(|_| Error::InvalidPointer)
		}

	}
	impl<T: CoreIoWrite> CoreIoWriteDispatch for T {
		fn dispatch_write(&self, a0: usize, a1: usize, a2: usize, a3: usize, a4: usize) -> Result<u128, Error> {
			let v0 = a0 as isize;
			let v1 = {
				let ptr = User::<*const u8>::new_in(
					a1 as _,
					AddressSpaceInner::to_api(
						percpu_v2!(current_thread).read().as_ref().expect("can only syscall from thread")
						                          .tcb_ref().address_space
					),
				);
				unsafe { slice_from_raw_parts(ptr, a2).read_to_buffer()? }
			}; let v1 = &*v1;
			let ret = <Self as CoreIoWrite>::write(self,
				v0,
				v1,
			)?;
			Ok(ret as u128)
		}
	}
	pub trait CoreIoWrite: CoreIoWriteDispatch {
		fn write(&self, handle: isize, buf: &[u8],
		) -> Result<usize, Error>;
		fn __vtable() -> HashMap<u128, fn(*const (), usize, usize, usize, usize, usize) -> Result<u128, Error>> where Self: Sized { let mut map = HashMap::new(); map.insert_unique_unchecked(3u128, unsafe { core::mem::transmute(<Self as CoreIoWriteDispatch>::deserialize_ctor as fn(_) -> _) }); map.insert_unique_unchecked(3u128 | (1 as u128) << 96, unsafe { core::mem::transmute(<Self as CoreIoWriteDispatch>::dispatch_write as fn(_, _, _, _, _, _) -> _) });
			map }
	}
	impl Protocol for dyn CoreIoWrite { const UID: u128 = 3; type Ctor<'a> = CoreIoWriteCtor<'a>; }
	#[repr(C)] pub struct CoreIoWriteCtor<'a> {
		_phantom: PhantomData<&'a ()> }
	pub trait CoreProcThreadDispatch {
		fn dispatch_unstable_anon_alloc(&self, a0: usize, a1: usize, a2: usize, a3: usize, a4: usize) -> Result<u128, Error>;
		fn dispatch_unstable_anon_dealloc(&self, a0: usize, a1: usize, a2: usize, a3: usize, a4: usize) -> Result<u128, Error>;
		fn dispatch_set_tcb(&self, a0: usize, a1: usize, a2: usize, a3: usize, a4: usize) -> Result<u128, Error>;

		fn deserialize_ctor(buffer: &mut User<*const u8>) -> Result<Box<[u8]>, Error> where Self: Sized {
			let ctor_args_layout = Layout::new::<CoreProcThreadCtor>();
			unsafe { *buffer = buffer.byte_offset(buffer.align_offset(ctor_args_layout.align()) as isize); }
			let bytes = slice_from_raw_parts(*buffer, ctor_args_layout.size());
			unsafe { *buffer = buffer.byte_offset(ctor_args_layout.size() as isize); }
			unsafe { bytes.read_to_buffer() }.map_err(|_| Error::InvalidPointer)
		}

	}
	impl<T: CoreProcThread> CoreProcThreadDispatch for T {
		fn dispatch_unstable_anon_alloc(&self, a0: usize, a1: usize, a2: usize, a3: usize, a4: usize) -> Result<u128, Error> {
			let v0 = a0 as isize;
			let v1 = a1 as usize;
			let ret = <Self as CoreProcThread>::unstable_anon_alloc(self,
				v0,
				v1,
			)?;
			Ok(ret as u128)
		}
		fn dispatch_unstable_anon_dealloc(&self, a0: usize, a1: usize, a2: usize, a3: usize, a4: usize) -> Result<u128, Error> {
			let v0 = a0 as isize;
			let v1 = a1 as *const u8;
			let ret = <Self as CoreProcThread>::unstable_anon_dealloc(self,
				v0,
				v1,
			)?;
			Ok(0)
		}
		fn dispatch_set_tcb(&self, a0: usize, a1: usize, a2: usize, a3: usize, a4: usize) -> Result<u128, Error> {
			let v0 = a0 as isize;
			let v1 = a1 as *const u8;
			let ret = <Self as CoreProcThread>::set_tcb(self,
				v0,
				v1,
			)?;
			Ok(0)
		}
	}
	pub trait CoreProcThread: CoreProcThreadDispatch {
		fn unstable_anon_alloc(&self, handle: isize, size: usize,
		) -> Result<*const u8, Error>;
		fn unstable_anon_dealloc(&self, handle: isize, pointer: *const u8,
		) -> Result<(), Error>;
		fn set_tcb(&self, handle: isize, pointer: *const u8,
		) -> Result<(), Error>;
		fn __vtable() -> HashMap<u128, fn(*const (), usize, usize, usize, usize, usize) -> Result<u128, Error>> where Self: Sized { let mut map = HashMap::new(); map.insert_unique_unchecked(10u128, unsafe { core::mem::transmute(<Self as CoreProcThreadDispatch>::deserialize_ctor as fn(_) -> _) }); map.insert_unique_unchecked(10u128 | (1 as u128) << 96, unsafe { core::mem::transmute(<Self as CoreProcThreadDispatch>::dispatch_unstable_anon_alloc as fn(_, _, _, _, _, _) -> _) });
			map.insert_unique_unchecked(10u128 | (2 as u128) << 96, unsafe { core::mem::transmute(<Self as CoreProcThreadDispatch>::dispatch_unstable_anon_dealloc as fn(_, _, _, _, _, _) -> _) });
			map.insert_unique_unchecked(10u128 | (3 as u128) << 96, unsafe { core::mem::transmute(<Self as CoreProcThreadDispatch>::dispatch_set_tcb as fn(_, _, _, _, _, _) -> _) });
			map }
	}
	impl Protocol for dyn CoreProcThread { const UID: u128 = 10; type Ctor<'a> = CoreProcThreadCtor<'a>; }
	#[repr(C)] pub struct CoreProcThreadCtor<'a> {
		_phantom: PhantomData<&'a ()> }

}
