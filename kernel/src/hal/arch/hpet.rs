#[repr(C)]
#[derive(Debug)]
pub struct Header {
	pub(super) capabilities: u64,
	_res0: u64,
	pub(super) configuration: u64,
	_res1: u64,
	pub(super) status: u64,
	_res2: [u64; 25],
	pub(super) counter: u64,
	_res3: u64,
}
