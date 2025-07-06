pub mod collections {
	pub type HashMap<K, V> = hashbrown::HashMap<K, V>;
}

pub mod os {
	pub mod popcorn {
		pub mod proto {
			pub use crate::ipc::Error;
			pub use crate::ipc::protocol::Protocol;
			
			pub mod server {
				pub use crate::ipc::server::MethodResult as Result;
				pub use crate::ipc::server::ReturnHandle as ReturnHandle;
			}
		}
		pub mod handle {
			use alloc::sync::Arc;
			use core::marker::PhantomData;
			use crate::ipc::handle::Handle;

			pub type OwnedHandle<T = ()> = Shim<T>::Handle;

			pub struct Shim<T>(PhantomData<T>);

			impl<T> Shim<T> {
				pub type Handle = Arc<Handle>;
			}

			pub struct RawHandle(pub isize);

			pub trait FromRawHandle {
				unsafe fn from_raw_handle(raw: RawHandle) -> Self;
			}

			impl FromRawHandle for Arc<Handle> {
				unsafe fn from_raw_handle(raw: RawHandle) -> Self {
					Self::from_raw(raw.0 as *const _)
				}
			}
		}
	}
}

pub mod path {
	pub type Path = str;
}
