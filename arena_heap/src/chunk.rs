use core::fmt::{Debug, Formatter};
use core::ptr::{NonNull, self};
#[cfg(feature = "generations")] use log::error;

const _: () = {
	assert!(align_of::<ChunkHeader>() >= 2, "chunks must be aligned to at least 2 bytes to allow for pointer tagging");
	#[cfg(feature = "generations")] assert!(align_of::<ChunkHeader>() >= 8, "chunks must be aligned to at least 8 bytes to allow for pointer tagging");
};

#[cfg(any(feature = "checksum", feature = "kasan"))] const MAGIC: usize = u64::from_ne_bytes(*b"POP HEAP") as usize;
const BUSY_MASK: usize = if cfg!(feature = "generations") { 7 } else { 1 };

#[cfg(feature = "generations")] const BUSY_VAL: usize = 7;
#[cfg(feature = "generations")] const FREE_VAL: usize = 0;
#[cfg(feature = "generations")] pub const MAGIC_2: usize = 0xcafebabe_deadbeefu64 as usize;
#[cfg(feature = "reuse-backtrace")] const FRAME_COUNT: usize = 35;

#[cfg(feature = "reuse-backtrace")]
unsafe extern "Rust" {
	#[link_name = "__popcorn_stack_trace_iter"]
	safe fn stack_trace_fn(ptr: fn(usize, *const u8), ctx: *const u8);
}

// Invariant: memory from self to next is valid and, if marked as free, unaliased
// Invariant: chunks form a valid doubly linked list in memory order
pub struct ChunkHeader {
	next: NextPtr,
	prev: Option<NonNull<ChunkHeader>>,
	#[cfg(feature = "checksum")] checksum: usize,
	#[cfg(feature = "reuse-backtrace")] free_backtrace: Option<[usize; FRAME_COUNT]>,
	#[cfg(all(feature = "kasan", not(feature = "reuse-backtrace")))] _redzone: [usize; 8],
}

impl Clone for ChunkHeader {
	#[cfg_attr(feature = "kasan", inline(never))]
	#[cfg_attr(feature = "kasan", sanitize(address = "off"))]
	fn clone(&self) -> Self {
		ChunkHeader {
			next: self.next.clone(),
			prev: self.prev,
			#[cfg(feature = "checksum")] checksum: self.checksum,
			#[cfg(feature = "reuse-backtrace")] free_backtrace: self.free_backtrace,
			#[cfg(all(feature = "kasan", not(feature = "reuse-backtrace")))] _redzone: self._redzone,
		}
	}
}

impl Debug for ChunkHeader {
	#[cfg_attr(feature = "kasan", inline(never))]
	#[cfg_attr(feature = "kasan", sanitize(address = "off"))]
	fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
		let mut d = f.debug_struct("ChunkHeader");
		d.field("busy", &self.next.busy());
		d.field("next", &self.next.ptr());
		d.field("prev", &self.prev);
		#[cfg(feature = "checksum")] d.field_with("checksum", |f| {
					write!(f, "{:#x} ", self.checksum)?;
					if self.checksum == MAGIC { write!(f, "(VALID)") }
					else { write!(f, "(INVALID)") }
				});
		d.finish()
	}
}

impl ChunkHeader {
	/// # Safety
	/// 
	/// The memory between `self` and `next` (if non-null) must be valid, unaliased memory
	/// 
	/// `prev` (if non-null) must point to a `ChunkHeader` that satisfies `prev.next == self`
	pub unsafe fn new(next: Option<NonNull<ChunkHeader>>, prev: Option<NonNull<ChunkHeader>>) -> Self {
		let next = match next {
			Some(ptr) => {
				assert!(ptr.is_aligned_to(2));
				#[cfg(feature = "generations")] assert!(ptr.is_aligned_to(8));
				NextPtr(ptr.as_ptr().map_addr(|p| p & !BUSY_MASK))
			},
			None => NextPtr(ptr::null_mut()),
		};
		Self {
			next,
			prev,
			#[cfg(feature = "checksum")] checksum: MAGIC,
			#[cfg(feature = "reuse-backtrace")] free_backtrace: None,
			#[cfg(all(feature = "kasan", not(feature = "reuse-backtrace")))] _redzone: [MAGIC; 8],
		}
	}

	#[cfg_attr(feature = "kasan", inline(never))]
	#[cfg_attr(feature = "kasan", sanitize(address = "off"))]
	pub fn busy(&self) -> bool {
		#[cfg(feature = "checksum")] {
		    let checksum = self.checksum; // to ensure mem access stays in no_san
		    assert_eq!(checksum, MAGIC, "checksum failed for {:#p}", self.start());
		}
		self.next.busy()
	}

	#[cfg(not(feature = "generations"))]
	pub fn set_busy(&mut self, busy: bool) {
		#[cfg(feature = "checksum")] {
		    let checksum = self.checksum; // to ensure mem access stays in no_san
		    assert_eq!(checksum, MAGIC, "checksum failed for {:#p}", self.start());
		}
		if !busy { assert!(self.busy(), "double free detected"); }
		self.next.set_busy(busy);
	}

	#[cfg_attr(feature = "kasan", inline(never))]
	#[cfg_attr(feature = "kasan", sanitize(address = "off"))]
	#[cfg(feature = "generations")]
	pub fn set_busy(&mut self, busy: bool) {
		#[cfg(feature = "checksum")] {
		    let checksum = self.checksum; // to ensure mem access stays in no_san
		    assert_eq!(checksum, MAGIC, "checksum failed for {:#p}", self.start());
		}

		if busy {
			self.next.set_generation(BUSY_VAL);
		} else {
			assert!(self.busy(), "double free detected");
			self.next.set_generation(BUSY_VAL - 1);
			let mut write = self.start().cast::<usize>();
			let end = self.end().cast::<usize>();
			while write != end {
				unsafe { *write.as_ptr() = MAGIC_2; }
				unsafe { write = write.offset(1); }
			}
			
			#[cfg(feature = "reuse-backtrace")] {
				let mut frames = [0; FRAME_COUNT];
				let mut ptr = frames.as_mut_ptr_range();
				let ptr = addr_of_mut!(ptr).cast_const().cast();
				stack_trace_fn(|ip, ctx| {
					let ptr = ctx.cast_mut().cast::<Range<*mut usize>>();
					if unsafe { (*ptr).start != (*ptr).end } {
						unsafe { *(*ptr).start = ip; }
						unsafe { (*ptr).start = (*ptr).start.offset(1); }
					}
				}, ptr);
				self.free_backtrace = Some(frames);
			}
		}
	}

	#[cfg_attr(feature = "kasan", inline(never))]
	#[cfg_attr(feature = "kasan", sanitize(address = "off"))]
	#[cfg(feature = "generations")]
	pub fn update_generations(&mut self) {
		#[cfg(feature = "checksum")] {
		    let checksum = self.checksum; // to ensure mem access stays in no_san
		    assert_eq!(checksum, MAGIC, "checksum failed for {:#p}", self.start());
		}
		match self.next.get_generation() {
			BUSY_VAL => {},
			FREE_VAL => {},
			v => {
				assert!(v > FREE_VAL && v < BUSY_VAL, "invalid generation detected for {:#p}", self.start());
				let mut read = self.start().cast::<usize>();
				let end = self.end().cast::<usize>();
				while read != end {
					if unsafe { *read.as_ptr() } != MAGIC_2 {
						error!("use after free detected for {:#p} ({:#x} != MAGIC)", self.start(), unsafe { *read.as_ptr() });
						#[cfg(feature = "reuse-backtrace")] if let Some(free_trace) = self.free_backtrace.as_ref() {
							error!("freed at:\n{free_trace:#x?}");
						}
						panic!();
					}
					unsafe { read = read.offset(1); }
				}
				self.next.set_generation(v - 1);
			}
		}
	}

	#[cfg_attr(feature = "kasan", inline(never))]
	#[cfg_attr(feature = "kasan", sanitize(address = "off"))]
	pub fn size(&self) -> usize {
		#[cfg(feature = "checksum")] {
		    let checksum = self.checksum; // to ensure mem access stays in no_san
		    assert_eq!(checksum, MAGIC, "checksum failed for {:#p}", self.start());
		}

		let next = self.next.ptr().expect("cannot get size of end node");
		let this = self as *const Self;
		let diff = unsafe { next.as_ptr().cast::<u8>().offset_from(this.cast::<u8>()) };
		assert!(diff > 0, "um why is there a negative sized allocation");
		
		diff.unsigned_abs() - size_of::<Self>()
	}

	#[cfg_attr(feature = "kasan", inline(never))]
	#[cfg_attr(feature = "kasan", sanitize(address = "off"))]
	pub fn next(&self) -> Option<NonNull<ChunkHeader>> {
		#[cfg(feature = "checksum")] {
		    let checksum = self.checksum; // to ensure mem access stays in no_san
		    assert_eq!(checksum, MAGIC, "checksum failed for {:#p}", self.start());
		}
		self.next.ptr()
	}

	#[cfg_attr(feature = "kasan", inline(never))]
	#[cfg_attr(feature = "kasan", sanitize(address = "off"))]
	pub fn set_next(&mut self, next: Option<NonNull<ChunkHeader>>) {
		#[cfg(feature = "checksum")] {
		    let checksum = self.checksum; // to ensure mem access stays in no_san
		    assert_eq!(checksum, MAGIC, "checksum failed for {:#p}", self.start());
		}
		self.next.set_ptr(next)
	}

	#[cfg_attr(feature = "kasan", inline(never))]
	#[cfg_attr(feature = "kasan", sanitize(address = "off"))]
	pub fn prev(&self) -> Option<NonNull<ChunkHeader>> {
		#[cfg(feature = "checksum")] {
		    let checksum = self.checksum; // to ensure mem access stays in no_san
		    assert_eq!(checksum, MAGIC, "checksum failed for {:#p}", self.start());
		}
		self.prev
	}

	#[cfg_attr(feature = "kasan", inline(never))]
	#[cfg_attr(feature = "kasan", sanitize(address = "off"))]
	pub fn set_prev(&mut self, prev: Option<NonNull<ChunkHeader>>) {
		#[cfg(feature = "checksum")] {
		    let checksum = self.checksum; // to ensure mem access stays in no_san
		    assert_eq!(checksum, MAGIC, "checksum failed for {:#p}", self.start());
		}
		self.prev = prev
	}

	#[cfg_attr(all(feature = "kasan", feature = "checksum"), inline(never))]
	#[cfg_attr(all(feature = "kasan", feature = "checksum"), sanitize(address = "off"))]
	// fixme(provenance): `self` has provenance bounds too small for any useful usecases of `start`
	// it needs to have a bound that covers the entire chunk, not just the header
	pub fn start(&self) -> NonNull<u8> {
		#[cfg(feature = "checksum")] {
		    let checksum = self.checksum; // to ensure mem access stays in no_san
		    assert_eq!(checksum, MAGIC, "checksum failed for {:#p}", self.start());
		}
		// SAFETY: returns a pointer one past the end
		unsafe { NonNull::from(self).cast::<u8>().byte_add(size_of::<Self>()) }
	}

	#[cfg_attr(feature = "kasan", inline(never))]
	#[cfg_attr(feature = "kasan", sanitize(address = "off"))]
	pub fn end(&self) -> NonNull<u8> {
		#[cfg(feature = "checksum")] {
		    let checksum = self.checksum; // to ensure mem access stays in no_san
		    assert_eq!(checksum, MAGIC, "checksum failed for {:#p}", self.start());
		}
		self.next.ptr().expect("cannot get end of sentinel node").cast()
	}

	#[cfg_attr(feature = "kasan", inline(never))]
	#[cfg_attr(feature = "kasan", sanitize(address = "off"))]
	#[cfg(feature = "generations")]
	pub fn overwrite_magic(&mut self) {
		let mut write = (self as *mut Self).cast::<usize>();
		let end = self.end().cast::<usize>();
		while write != end.as_ptr() {
			unsafe { *write = MAGIC_2; }
			unsafe { write = write.offset(1); }
		}
	}
}

struct NextPtr(*mut ChunkHeader);

impl Clone for NextPtr {
	#[cfg_attr(feature = "kasan", inline(never))]
	#[cfg_attr(feature = "kasan", sanitize(address = "off"))]
	fn clone(&self) -> Self {
		Self(self.0.clone())
	}
}

impl NextPtr {
	#[cfg_attr(feature = "kasan", sanitize(address = "off"))]
	fn ptr(&self) -> Option<NonNull<ChunkHeader>> {
		NonNull::new(self.0.map_addr(|val| val & !BUSY_MASK))
	}

	#[cfg_attr(feature = "kasan", sanitize(address = "off"))]
	fn get_generation(&self) -> usize {
		self.0.addr() & BUSY_MASK
	}

	#[cfg_attr(feature = "kasan", sanitize(address = "off"))]
	fn busy(&self) -> bool {
		let val = self.get_generation();
		#[cfg(not(feature = "generations"))] let res = val != 0;
		#[cfg(feature = "generations")] let res = match val {
			FREE_VAL => false,
			_ => true,
		};
		res
	}

	#[cfg_attr(feature = "kasan", sanitize(address = "off"))]
	fn set_ptr(&mut self, val: Option<NonNull<ChunkHeader>>) {
		let mut wrapped = NextPtr(val.map_or(ptr::null_mut(), |p| p.as_ptr()));
		#[cfg(not(feature = "generations"))] wrapped.set_busy(self.busy());
		#[cfg(feature = "generations")] wrapped.set_generation(self.get_generation());
		*self = wrapped;
	}

	#[cfg(not(feature = "generations"))]
	fn set_busy(&mut self, is_busy: bool) {
		self.0 = self.0.map_addr(|val| (val & !BUSY_MASK) | (is_busy as usize));
	}

	#[cfg_attr(feature = "kasan", sanitize(address = "off"))]
	#[cfg(feature = "generations")]
	fn set_generation(&mut self, generation: usize) {
		assert!(generation <= BUSY_VAL);
		self.0 = self.0.map_addr(|val| (val & !BUSY_MASK) | generation);
	}
}

impl Debug for NextPtr {
	#[cfg_attr(feature = "kasan", inline(never))]
	#[cfg_attr(feature = "kasan", sanitize(address = "off"))]
	fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
		f.debug_tuple("NextPtr")
				.field(&self.ptr())
				.field(&self.busy())
				.finish()
	}
}
