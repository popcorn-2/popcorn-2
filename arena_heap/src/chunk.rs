use core::fmt::{Debug, Formatter};
use core::ptr::{NonNull, self};

const _: () = {
	assert!(align_of::<ChunkHeader>() >= 2, "chunks must be aligned to at least 2 bytes to allow for pointer tagging");	
};

#[derive(Debug)]
// Invariant: memory from self to next is valid and, if marked as free, unaliased
// Invariant: chunks form a valid doubly linked list in memory order
pub struct ChunkHeader {
	next: NextPtr,
	prev: Option<NonNull<ChunkHeader>>,
}

impl ChunkHeader {
	/// # Safety
	/// 
	/// The memory between `self` and `next` (if non-null) must be valid, unaliased memory
	/// 
	/// `prev` (if non-null) must point to a `ChunkHeader` that satisfies `prev.next == self`
	pub unsafe fn new(next: Option<NonNull<ChunkHeader>>, prev: Option<NonNull<ChunkHeader>>) -> Self {
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
	pub fn set_busy(&mut self, busy: bool) { self.next.set_busy(busy); }
	
	pub fn size(&self) -> usize {
		let next = self.next.ptr().expect("cannot get size of end node").addr().get();
		let this = (self as *const Self).addr();
		
		next - this - size_of::<Self>()
	}
	
	pub fn next(&self) -> Option<NonNull<ChunkHeader>> {
		self.next.ptr()
	}
	
	pub fn set_next(&mut self, next: Option<NonNull<ChunkHeader>>) {
		self.next.set_ptr(next)
	}

	pub fn prev(&self) -> Option<NonNull<ChunkHeader>> {
		self.prev
	}

	pub fn set_prev(&mut self, prev: Option<NonNull<ChunkHeader>>) {
		self.prev = prev
	}
	
	// fixme(provenance): `self` has provenance bounds too small for any useful usecases of `start`
	// it needs to have a bound that covers the entire chunk, not just the header
	pub fn start(&self) -> NonNull<u8> {
		// SAFETY: returns a pointer one past the end
		unsafe { NonNull::from(self).cast::<u8>().byte_add(size_of::<Self>()) }
	}

	pub fn end(&self) -> NonNull<u8> {
		self.next.ptr().expect("cannot get end of sentinel node").cast()
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
