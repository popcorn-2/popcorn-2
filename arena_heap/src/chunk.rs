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
	pub fn new(next: Option<NonNull<ChunkHeader>>, prev: Option<NonNull<ChunkHeader>>) -> Self {
		let next = match next {
			Some(ptr) => NextPtr(ptr.as_ptr().map_addr(|p| p & !1)),
			None => NextPtr(ptr::null_mut()),
		};
		Self {
			next,
			prev,
		}
	}

	pub fn busy(&self) -> bool { self.next.busy() }
	
	pub fn size(&self) -> usize {
		let next = self.next.ptr().expect("cannot get size of end node").addr().get();
		let this = (self as *const Self).addr();
		
		next - this - size_of::<Self>()
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
