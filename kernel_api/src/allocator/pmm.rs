use core::marker::PhantomData;
use core::num::NonZero;
use crate::memory::{Frames, RawFrame};
use crate::sync::RwSpinlock;
use crate::allocator::AllocError;

/// Returns the kernel's `highmem` allocator.
/// 
/// This should be the default allocator when allocating physical memory.
///
/// See the [memory module docs](`crate::memory#physical`) for more information.
#[inline]
#[must_use]
pub const fn highmem() -> DynPmm<'static, true> {
	DynPmm::from(&crate::bridge::memory::GLOBAL_HIGHMEM)
}

/// Returns the kernel's `dmamem` allocator.
///
/// This should be used sparingly and only when allocating DMA memory for 32-bit
/// DMA controllers.
///
/// See the [memory module docs](`crate::memory#physical`) for more information.
#[inline]
#[must_use]
pub const fn dmamem() -> DynPmm<'static, true> {
	DynPmm::from(&crate::bridge::memory::GLOBAL_DMA)
}

/// Returns an allocator which uses the [`highmem`](`crate::memory#highmem`) allocator as a backing allocator but zeroes all memory.
#[inline]
#[must_use]
pub const fn highmem_zero() -> DynPmm<'static, true> {
	struct Zero;

	// SAFETY: All allocated memory comes from `highmem()` which implements `Pmm<true>`
	unsafe impl Pmm<true> for Zero {
		fn allocate_one_raw(&self) -> Result<RawFrame, AllocError> {
			let mut frame = highmem().allocate_one()?;
			frame.write_filled(0);
			Ok(Frames::into_raw(frame).0.start)
		}

		fn allocate_raw(&self, count: NonZero<usize>) -> Result<RawFrame, AllocError> {
			let mut frames = highmem().allocate(count)?;
			frames.write_filled(0);
			Ok(Frames::into_raw(frames).0.start)
		}

		fn allocate_raw_at(&self, at: RawFrame, count: NonZero<usize>) -> Result<RawFrame, AllocError> {
			let mut frames = highmem().allocate_at(at, count)?;
			frames.write_filled(0);
			Ok(Frames::into_raw(frames).0.start)
		}

		unsafe fn deallocate_raw(&self, base: RawFrame, count: NonZero<usize>) {
			// SAFETY: safety requirements of `deallocate_raw` are upheld by caller
			unsafe { highmem().deallocate_raw(base, count) }
		}
	}
	
	DynPmm::from(&Zero)
}

/// A physical memory allocator.
/// 
/// If the allocator only allocates from conventional RAM, then the
/// `RAM_ONLY` flag is set, and any returned memory can be accessed via
/// the [page map](crate::memory#page-map-region).
/// 
/// # Safety
/// 
/// The implementation must return unaliased physical memory.
/// Additionally, if `RAM_ONLY` is `true`, the returned memory ranges must
/// only be from conventional memory, i.e. `EfiConventionalMemory`.
pub unsafe trait Pmm<const RAM_ONLY: bool>: Sync + Sized { // todo: can we remove the Sync bound? it makes dyn stuff a real pain
	                                                       // the Sized bound is chucked on to prevent `dyn Pmm` being used
	/// Allocates a single frame.
	///
	/// # Errors
	///
	/// Returns an [`AllocError`] if the memory could not be allocated.
	#[must_use = "memory will be leaked unless the frame is explicitly deallocated"]
	fn allocate_one_raw(&self) -> Result<RawFrame, AllocError> { self.allocate_raw(const { NonZero::new(1).unwrap() }) }

	/// Allocates `count` number of contiguous frames.
	///
	/// # Errors
	///
	/// Returns an [`AllocError`] if the memory could not be allocated. This
	/// does not mean there is no free memory, just that is there no region
	/// of contiguous memory large enough.
	#[must_use = "memory will be leaked unless the frame is explicitly deallocated"]
	fn allocate_raw(&self, count: NonZero<usize>) -> Result<RawFrame, AllocError>;

	/// Allocates `count` number of contiguous frames with the first frame being at `at`.
	///
	/// # Errors
	///
	/// Returns an [`AllocError`] if the memory could not be allocated. This
	/// does not mean there is no free memory, just that is there no region
	/// of contiguous memory large enough starting at `at`.
	#[must_use = "memory will be leaked unless the frame is explicitly deallocated"]
	fn allocate_raw_at(&self, at: RawFrame, count: NonZero<usize>) -> Result<RawFrame, AllocError>;

	/// Deallocates `count` frames starting at `base`.
	/// 
	/// # Safety
	/// 
	/// All frames in the range `base .. (base + count)` must have been allocated by this allocator.
	unsafe fn deallocate_raw(&self, base: RawFrame, count: NonZero<usize>);
}

// SAFETY: this implementation only exists where `Pmm` has already been implemented for T
unsafe impl<const RAM_ONLY: bool, T: Pmm<RAM_ONLY>> Pmm<RAM_ONLY> for &T {
	fn allocate_one_raw(&self) -> Result<RawFrame, AllocError> {
		(**self).allocate_one_raw()
	}
	
	fn allocate_raw(&self, count: NonZero<usize>) -> Result<RawFrame, AllocError> {
		(**self).allocate_raw(count)
	}

	fn allocate_raw_at(&self, at: RawFrame, count: NonZero<usize>) -> Result<RawFrame, AllocError> {
		(**self).allocate_raw_at(at, count)
	}

	unsafe fn deallocate_raw(&self, base: RawFrame, count: NonZero<usize>) {
		// SAFETY: this function has the same safety requirements as `deallocate_raw`
		unsafe { (**self).deallocate_raw(base, count) }
	}
}

#[derive(Debug)]
#[doc(hidden)]
pub struct GlobalAllocator {
	#[doc(hidden)]
	pub __rwlock: RwSpinlock<Option<DynPmm<'static, true>>>
}

// SAFETY: all calls are forwarded to a type that implements `Pmm` itself
unsafe impl Pmm<true> for GlobalAllocator {
	#[inline]
	fn allocate_raw(&self, count: NonZero<usize>) -> Result<RawFrame, AllocError> {
		self.__rwlock.read()
				.expect("no global Pmm")
				.allocate_raw(count)
	}

	#[inline]
	fn allocate_raw_at(&self, at: RawFrame, count: NonZero<usize>) -> Result<RawFrame, AllocError> {
		self.__rwlock.read()
		    .expect("no global Pmm")
		    .allocate_raw_at(at, count)
	}

	#[inline]
	unsafe fn deallocate_raw(&self, base: RawFrame, count: NonZero<usize>) {
		let pmm = self.__rwlock.read()
		    .expect("no global Pmm");

		// SAFETY: this function has the same safety requirements as `pmm.deallocate_raw`
		unsafe { pmm.deallocate_raw(base, count) }
	}
}

/// A type erased reference to a [`Pmm`].
///
/// This is equivalent to `&dyn Pmm` with the addition that a `DynPmm<true>` can be converted directly
/// to a `DynPmm<false>` by calling [`from`](`From::from`) or [`into`](`Into::into`).
#[derive(Copy, Clone, Debug)] // this is effectively a `&dyn Pmm` therefore we can copy it like a `&` ref
#[repr(C)] // to force consistent layout between RAM and !RAM variants
// This is used instead of `&'a dyn Pmm<RAM>` to ensure vtable layout is the same between RAM and !RAM
// versions so we can freely interconvert
// INVARIANT: `data` points to a valid implementation of `Pmm<RAM>` with lifetime 'a
// INVARIANT: all function pointers point to the implementation's corresponding method
pub struct DynPmm<'a, const RAM: bool> {
	data: *const (), // we could use NonNull here but we already get niche optimization from all the fn ptrs
	allocate_raw_at: unsafe fn(*const (), at: RawFrame, count: NonZero<usize>) -> Result<RawFrame, AllocError>,
	allocate_one_raw: unsafe fn(*const ()) -> Result<RawFrame, AllocError>,
	allocate_raw: unsafe fn(*const (), count: NonZero<usize>) -> Result<RawFrame, AllocError>,
	deallocate_raw: unsafe fn(*const (), base: RawFrame, count: NonZero<usize>),
	_phantom: PhantomData<&'a ()>,
}

// SAFETY: all implementations of Pmm are Sync, therefore &Pmm (which DynPmm is equivalent to) is Sync
unsafe impl<const RAM: bool> Sync for DynPmm<'_, RAM> {}
// SAFETY: all implementations of Pmm are Sync, therefore &Pmm (which DynPmm is equivalent to) is Send
unsafe impl<const RAM: bool> Send for DynPmm<'_, RAM> {}

impl<'a, const RAM: bool, T: Pmm<RAM>> const From<&'a T> for DynPmm<'a, RAM> {
	fn from(value: &'a T) -> Self {
		/// # Safety
		/// `this` must be valid to convert to a reference to `T`.
		/// # Errors
		/// See [`Pmm::allocate_raw_at`].
		#[inline]
		unsafe fn allocate_raw_at_shim<const RAM: bool, T: Pmm<RAM>>(this: *const (), at: RawFrame, count: NonZero<usize>) -> Result<RawFrame, AllocError> {
			// SAFETY: `this` is valid to convert to ref to `T`, upheld by caller
			T::allocate_raw_at(unsafe { &*this.cast() }, at, count)
		}

		/// # Safety
		/// `this` must be valid to convert to a reference to `T`.
		/// # Errors
		/// See [`Pmm::allocate_one_raw`].
		#[inline]
		unsafe fn allocate_one_raw_shim<const RAM: bool, T: Pmm<RAM>>(this: *const ()) -> Result<RawFrame, AllocError> {
			// SAFETY: `this` is valid to convert to ref to `T`, upheld by caller
			T::allocate_one_raw(unsafe { &*this.cast() })
		}

		/// # Safety
		/// `this` must be valid to convert to a reference to `T`.
		/// # Errors
		/// See [`Pmm::allocate_raw`].
		#[inline]
		unsafe fn allocate_raw_shim<const RAM: bool, T: Pmm<RAM>>(this: *const (), count: NonZero<usize>) -> Result<RawFrame, AllocError> {
			// SAFETY: `this` is valid to convert to ref to `T`, upheld by caller
			T::allocate_raw(unsafe { &*this.cast() }, count)
		}

		/// # Safety
		/// `this` must be valid to convert to a reference to `T`, and all safety requirements of `Pmm::deallocate_raw`
		/// must be upheld.
		#[inline]
		unsafe fn deallocate_raw_shim<const RAM: bool, T: Pmm<RAM>>(this: *const (), base: RawFrame, count: NonZero<usize>) {
			// SAFETY: `this` is valid to convert to ref to `T`, upheld by caller
			let this = unsafe { &*this.cast() };
			// SAFETY: safety requirements of `deallocate_raw` upheld by caller
			unsafe { T::deallocate_raw(this, base, count) }
		}

		DynPmm {
			data: core::ptr::from_ref(value).cast(),
			allocate_raw_at: allocate_raw_at_shim::<RAM, T>,
			allocate_one_raw: allocate_one_raw_shim::<RAM, T>,
			allocate_raw: allocate_raw_shim::<RAM, T>,
			deallocate_raw: deallocate_raw_shim::<RAM, T>,
			_phantom: PhantomData,
		}
	}
}

impl<'a, const RAM: bool> DynPmm<'a, RAM> {
	/// Converts to a `DynPmm<false>`.
	#[must_use]
	#[expect(clippy::unnecessary_struct_initialization, reason = "false positive since it changes type param")]
	pub const fn into(self) -> DynPmm<'a, false> {
		DynPmm::<false> {
			..self
		}
	}

	/// Allocates a single owned [frame](`Frames`).
	///
	/// # Errors
	///
	/// Returns an [`AllocError`] if the memory could not be allocated.
	pub fn allocate_one(&self) -> Result<Frames<RAM>, AllocError> where Self: 'static {
		let base = self.allocate_one_raw()?;
		// SAFETY: `base` was just allocated by `self`
		Ok(unsafe {
			Frames::from_raw(
				base .. (base + 1usize),
				*self,
			)
		})
	}

	/// Allocates `counts` number of contiguous owned [`Frames`].
	///
	/// # Errors
	///
	/// Returns an [`AllocError`] if the memory could not be allocated. This
	/// does not mean there is no free memory, just that is there no region
	/// of contiguous memory large enough.
	pub fn allocate(&self, count: NonZero<usize>) -> Result<Frames<RAM>, AllocError> where Self: 'static {
		let base = self.allocate_raw(count)?;
		// SAFETY: `base` was just allocated by `self`
		Ok(unsafe {
			Frames::from_raw(
				base .. (base + count.get()),
				*self,
			)
		})
	}

	/// Allocates `count` number of contiguous owned [`Frames`] with the first frame being at `at`.
	///
	/// # Errors
	///
	/// Returns an [`AllocError`] if the memory could not be allocated. This
	/// does not mean there is no free memory, just that is there no region
	/// of contiguous memory large enough starting at `at`.
	pub fn allocate_at(&self, at: RawFrame, count: NonZero<usize>) -> Result<Frames<RAM>, AllocError> where Self: 'static {
		let base = self.allocate_raw_at(at, count)?;
		// SAFETY: `base` was just allocated by `self`
		Ok(unsafe {
			Frames::from_raw(
				base .. (base + count.get()),
				*self,
			)
		})
	}
}

// SAFETY: DynPmm<RAM> can only be constructed from a reference to an object
//  already implementing DynPmm<RAM>
unsafe impl<const RAM: bool> Pmm<RAM> for DynPmm<'_, RAM> {
	#[inline]
	fn allocate_one_raw(&self) -> Result<RawFrame, AllocError> {
		// SAFETY: invariant of `DynPmm` that `self.data` is type erased reference to same type as fn pointer is from
		unsafe { (self.allocate_one_raw)(self.data) }
	}

	#[inline]
	fn allocate_raw(&self, count: NonZero<usize>) -> Result<RawFrame, AllocError> {
		// SAFETY: invariant of `DynPmm` that `self.data` is type erased reference to same type as fn pointer is from
		unsafe { (self.allocate_raw)(self.data, count) }
	}

	#[inline]
	fn allocate_raw_at(&self, at: RawFrame, count: NonZero<usize>) -> Result<RawFrame, AllocError> {
		// SAFETY: invariant of `DynPmm` that `self.data` is type erased reference to same type as fn pointer is from
		unsafe { (self.allocate_raw_at)(self.data, at, count) }
	}

	#[inline]
	unsafe fn deallocate_raw(&self, base: RawFrame, count: NonZero<usize>) {
		// SAFETY: invariants of `self.deallocate_raw` are same as `Pmm::deallocate_raw`, and
		//  invariant of `DynPmm` that `self.data` is type erased reference to same type as fn pointer is from
		unsafe { (self.deallocate_raw)(self.data, base, count) }
	}
}
