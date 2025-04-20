#[allow(unused_imports)] use crate::prelude::*;
use utils::better_cow::Cow;
use crate::ipc::Error;
use crate::ipc::server::Server;

/// Manages thread objects, implementing `core.proc.Proc` and `core.proc.Thread`
/// 
/// Objects are implicitly opened by thread creation
/// 
/// Handle value is equal to [`ThreadId`](crate::threading::ThreadId)
#[derive(Debug)]
pub struct ProcServer {}

impl Server for ProcServer {
	fn open(&self, _endpoint: Cow<'_, Box<str>, str>) -> Result<usize, Error> {
		unimplemented!()
	}
}

impl ProcServer {
	pub const fn new() -> Self {
		Self {}
	}
}
