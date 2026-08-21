use core::marker::PhantomData;
use core::num::NonZero;
use crate::memory::{Frames, RawFrame};
use crate::sync::RwSpinlock;
use crate::allocator::AllocError;

/// Returns the kernel's `highmem` allocator
/// 
/// This should be the default allocator when allocating physical memory
#[inline]
pub const fn highmem() -> DynPmm<'static, true> {
	DynPmm::from(&crate::bridge::memory::GLOBAL_HIGHMEM)
}

/// Returns the kernel's `dmamem` allocator
///
/// This should be used sparingly and only when allocating DMA memory for 32-bit
/// DMA controllers.
#[inline]
pub const fn dmamem() -> DynPmm<'static, true> {
	DynPmm::from(&crate::bridge::memory::GLOBAL_DMA)
}

/// Returns an allocator which uses the [`highmem`](highmem()) allocator as a backing allocator but zeroes all memory
#[inline]
pub const fn highmem_zero() -> DynPmm<'static, true> {
	struct Zero;

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
			unsafe { highmem().deallocate_raw(base, count) }
		}
	}
	
	DynPmm::from(&Zero)
}

/// A physical memory allocator
/// 
/// If the allocator only allocates from conventional RAM, then the
/// `RAM_ONLY` flag is set, and any returned memory can be accessed via
/// the [page map](crate::memory#page-map-region)
/// 
/// # Safety
/// 
/// The implementation must return unaliased physical memory.
/// Additionally, if `RAM_ONLY` is `true`, the returned memory ranges must
/// only be from conventional memory, i.e. `EfiConventionalMemory`
pub unsafe trait Pmm<const RAM_ONLY: bool>: Sync + Sized { // todo: can we remove the Sync bound? it makes dyn stuff a real pain
	                                                       // the Sized bound is chucked on to prevent `dyn Pmm` being used
	/// Allocates a single frame
	///
	/// # Errors
	///
	/// Returns an [`AllocError`] if the memory could not be allocated
	fn allocate_one_raw(&self) -> Result<RawFrame, AllocError> { self.allocate_raw(const { NonZero::new(1).unwrap() }) }

	/// Allocates `count` number of contiguous frames
	///
	/// # Errors
	///
	/// Returns an [`AllocError`] if the memory could not be allocated. This
	/// does not mean there is no free memory, just that is there no region
	/// of contiguous memory large enough
	fn allocate_raw(&self, count: NonZero<usize>) -> Result<RawFrame, AllocError>;

	fn allocate_raw_at(&self, at: RawFrame, count: NonZero<usize>) -> Result<RawFrame, AllocError>;

	/// Deallocates `count` frames starting at `base`
	/// 
	/// # Safety
	/// 
	/// All frames in the range `base .. (count + base)` must have been allocated by this allocator
	unsafe fn deallocate_raw(&self, base: RawFrame, count: NonZero<usize>);
}

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
		unsafe { (**self).deallocate_raw(base, count) }
	}
}

#[doc(hidden)]
pub struct GlobalAllocator {
	pub __rwlock: RwSpinlock<Option<DynPmm<'static, true>>>
}

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

		unsafe { pmm.deallocate_raw(base, count) }
	}
}

#[derive(Copy, Clone, Debug)] // this is effectively a `&dyn Pmm` therefore we can copy it like a `&` ref
#[repr(C)] // to force consistent layout between RAM and !RAM variants
pub struct DynPmm<'a, const RAM: bool> {
	data: *const (), // we could use NonNull here but we already get niche optimization from all the fn ptrs
	allocate_raw_at: unsafe fn(*const (), at: RawFrame, count: NonZero<usize>) -> Result<RawFrame, AllocError>,
	allocate_one_raw: unsafe fn(*const ()) -> Result<RawFrame, AllocError>,
	allocate_raw: unsafe fn(*const (), count: NonZero<usize>) -> Result<RawFrame, AllocError>,
	deallocate_raw: unsafe fn(*const (), base: RawFrame, count: NonZero<usize>),
	_phantom: PhantomData<&'a ()>,
}

unsafe impl<const RAM: bool> Sync for DynPmm<'_, RAM> {}
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
	pub const fn into(self) -> DynPmm<'a, false> {
		DynPmm::<false> {
			..self
		}
	}

	/// Allocates a single owned [frame](Frames)
	///
	/// # Errors
	///
	/// Returns an [`AllocError`] if the memory could not be allocated
	pub fn allocate_one(&self) -> Result<Frames<RAM>, AllocError> where Self: 'static {
		let base = self.allocate_one_raw()?;
		Ok(unsafe {
			Frames::from_raw(
				base .. (base + 1usize),
				*self,
			)
		})
	}

	/// Allocates a `counts` contiguous owned [`Frames`]
	///
	/// # Errors
	///
	/// Returns an [`AllocError`] if the memory could not be allocated. This
	/// does not mean there is no free memory, just that is there no region
	/// of contiguous memory large enough
	pub fn allocate(&self, count: NonZero<usize>) -> Result<Frames<RAM>, AllocError> where Self: 'static {
		let base = self.allocate_raw(count)?;
		Ok(unsafe {
			Frames::from_raw(
				base .. (base + count.get()),
				*self,
			)
		})
	}

	pub fn allocate_at(&self, at: RawFrame, count: NonZero<usize>) -> Result<Frames<RAM>, AllocError> where Self: 'static {
		let base = self.allocate_raw_at(at, count)?;
		Ok(unsafe {
			Frames::from_raw(
				base .. (base + count.get()),
				*self,
			)
		})
	}
}

unsafe impl<const RAM: bool> Pmm<RAM> for DynPmm<'_, RAM> {
	#[inline]
	fn allocate_one_raw(&self) -> Result<RawFrame, AllocError> {
		unsafe { (self.allocate_one_raw)(self.data) }
	}

	#[inline]
	fn allocate_raw(&self, count: NonZero<usize>) -> Result<RawFrame, AllocError> {
		unsafe { (self.allocate_raw)(self.data, count) }
	}

	#[inline]
	fn allocate_raw_at(&self, at: RawFrame, count: NonZero<usize>) -> Result<RawFrame, AllocError> {
		unsafe { (self.allocate_raw_at)(self.data, at, count) }
	}

	#[inline]
	unsafe fn deallocate_raw(&self, base: RawFrame, count: NonZero<usize>) {
		unsafe { (self.deallocate_raw)(self.data, base, count) }
	}
}
