use alloc::sync::{Arc, Weak};
use crate::ipc::handle::{Handle, HandleMap, ServerId};
use kernel_api::ptr::{slice_from_raw_parts_mut, User};
use crate::ipc::protocol::meta;
use super::Error;

#[derive(Clone, Debug)]
pub enum Arg {
	Primitive(usize),
	BufferOffset(usize),
	NewBuffer,
	Handle(Arc<Handle>),
}

pub async fn serialize(
	value: Return,
	return_ptr: RawUserPointer<[u8]>,
	_expected_ret: meta::ReturnArg,
	default_protocol: Option<&[u128]>,
	server_id: ServerId,
	handle_map: Weak<HandleMap>,
) -> Result<u128, Error> {
	Ok(match value {
		Return::Boxed(boxed) => {
			let ret = with_thread_meta(|meta| {
				let return_ptr = { 
					let ptr = User::<*mut u8>::new_in(return_ptr.0, meta);
					slice_from_raw_parts_mut(ptr, return_ptr.1)
				};
				return_ptr.write_from_slice(&boxed)
			}).await;
			ret? as u128
		}
		Return::Value(val) => val,
		Return::Handle(handle) => {
			if let Some(handle_map) = handle_map.upgrade() {
				let res = handle_map.push(handle);
				res.map(|val| val as u128)?
			} else {
				Err(Error::DeadThread)?
			}
		}
		Return::NewHandle(handle, protos) => {
			let handle = Handle::new(
				server_id,
				handle,
				&*protos
			);
			if let Some(handle_map) = handle_map.upgrade() {
				let res = handle_map.push(handle);
				res.map(|val| val as u128)?
			} else {
				Err(Error::DeadThread)?
			}
		}
		Return::NewDefaultHandle(handle) => {
			let Some(default_protocol) = default_protocol else { unreachable!("server returned default protocol for non-ctor"); };
			let handle = Handle::new(
				server_id,
				handle,
				default_protocol,
			);
			if let Some(handle_map) = handle_map.upgrade() {
				let res = handle_map.push(handle);
				res.map(|val| val as u128)?
			} else {
				Err(Error::DeadThread)?
			}
		}
	})
}
