use core::fmt::{Debug, Formatter};
use core::ptr::{NonNull, self};

const _: () = {
	assert!(align_of::<ChunkHeader>() >= 2, "chunks must be aligned to at least 2 bytes to allow for pointer tagging");	
};

#[derive(Debug)]
pub struct ChunkHeader {
	next: NextPtr,
	prev: Option<NonNull<ChunkHeader>>,
}

impl ChunkHeader {
	pub fn new() -> Self {
		Self {
			next: NextPtr(ptr::null_mut()),
			prev: None,
		}
	}
}

struct NextPtr(*mut ChunkHeader);

impl NextPtr {
	fn ptr(&self) -> Option<NonNull<ChunkHeader>> {
		NonNull::new(self.0.map_addr(|val| val & !1))
	}
	
	fn busy(&self) -> bool {
		(self.0.addr() & 1) != 0
	}
	
	fn set_ptr(&mut self, val: Option<NonNull<ChunkHeader>>) {
		let mut wrapped = NextPtr(val.map_or(ptr::null_mut(), |p| p.as_ptr()));
		wrapped.set_busy(self.busy());
		*self = wrapped;
	}
	
	fn set_busy(&mut self, is_busy: bool) {
		self.0 = self.0.map_addr(|val| (val & !1) | (is_busy as usize));
	}
}

impl Debug for NextPtr {
	fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
		f.debug_tuple("NextPtr")
				.field(&self.ptr())
				.field(&self.busy())
				.finish()
	}
}
