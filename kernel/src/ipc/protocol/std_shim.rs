pub mod collections {
	pub type HashMap<K, V> = hashbrown::HashMap<K, V>;
}

pub mod os {
	pub mod popcorn {
		pub mod proto {
			pub use kernel_api::syscall::Error;
			pub use crate::ipc::protocol::Protocol;
		}
		pub mod handle {
			use alloc::sync::Arc;
			use core::marker::PhantomData;
			use kernel_api::syscall::handle::Handle;

			pub type OwnedHandle<T = ()> = <Shim<T> as ShimExt>::Handle;

			pub struct Shim<T>(PhantomData<T>);
			
			pub trait ShimExt {
				type Handle;
			}

			impl<T> ShimExt for Shim<T> {
				type Handle = Arc<Handle>;
			}

			pub struct RawHandle(pub isize);

			pub trait FromRawHandle {
				unsafe fn from_raw_handle(raw: RawHandle) -> Self;
			}

			impl FromRawHandle for Arc<Handle> {
				unsafe fn from_raw_handle(raw: RawHandle) -> Self {
					unsafe { Self::from_raw(raw.0 as *const _) }
				}
			}
		}
	}
}

pub mod path {
	pub type Path = str;
}
