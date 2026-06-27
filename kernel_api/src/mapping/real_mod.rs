use alloc::sync::Arc;
use core::fmt::{Debug, Formatter};
use crate::address_space;
use crate::address_space::MapPageError;
use core::mem::ManuallyDrop;
use core::num::NonZero;
use core::ops::Range;
use log::{debug, info, warn};
use crate::allocator::{AllocError, DynPmm};
use crate::memory::{Frames, RawFrame, RawPage, PAGE_SIZE};
use crate::ptr::User;
use crate::syscall::handle::Handle;

/// Used to track if the memory underlying the mapping is contiguous.
enum Backing {
    /// The underlying physical memory is contiguous, and starts at the contained frame.
    Contiguous(Frames<false>),

    /// The underlying physical memory is discontiguous, but all allocated by the same.
    Discontiguous { pmm: DynPmm<'static, false>, frame_count: usize },

    /// The mapping is backed by a VMO handle.
    Vmo { handle: Arc<Handle>, frame_count: usize },
}

impl Backing {
    const fn frame_len(&self) -> usize {
        match self {
            Self::Contiguous(frames) => frames.count(),
            Self::Discontiguous { frame_count, .. } | Self::Vmo { frame_count, .. } => *frame_count,
        }
    }

    const fn byte_len(&self) -> usize {
        self.frame_len() * PAGE_SIZE
    }

    fn pmm(&self) -> &DynPmm<'static, false> {
        match self {
            Self::Contiguous(frames) => frames.pmm(),
            Self::Discontiguous { pmm, .. } => pmm,
            Self::Vmo { .. } => unimplemented!("vmo does not have a pmm"),
        }
    }
}

/// An owned memory mapping.
///
/// This will allocate any required memory when created, and register any lazily mapped memory as such.
/// It will also manage the page tables to correctly unmap the memory when dropped, and deallocate any
/// memory previously allocated.
///
/// See the [module level documentation](`self`) for more information.
pub struct Mapping<R: Mappable, A: address_space::Ty> {
    raw: R,

    /// The address space mapped into.
    address_space: ManuallyDrop<A>,

    backing: ManuallyDrop<Backing>,
    //#[cfg(feature = "use_std")] backing: *mut libc::c_void,

    caching: Caching,

    virtual_start: RawPage,

    /// The protection used when mapping pages into this mapping.
    protection: Protection,
}

impl<R: Mappable> Mapping<R, address_space::Kernel> {
	/// Returns the two unsafe mutable pointers that span the range of the mapping.
	///
	/// The returned range is half-open, which means that the end pointer points *one past* the last element of the slice.
	/// This way, an empty slice is represented by two equal pointers, and the difference between the two pointers
	/// represents the size of the slice.
	#[must_use]
	pub fn as_mut_ptr_range(&mut self) -> Range<*mut u8> {
        let start = self.virtual_valid_start().as_ptr();
        Range {
            start,
	        // SAFETY: `start` points to the beginning of an allocation which is `self.byte_len` bytes long
            end: unsafe { start.byte_add(self.byte_len()) }
        }
    }

	/// Returns the two unsafe pointers that span the range of the mapping.
	///
	/// The returned range is half-open, which means that the end pointer points *one past* the last element of the slice.
	/// This way, an empty slice is represented by two equal pointers, and the difference between the two pointers
	/// represents the size of the slice.
	#[must_use]
	pub fn as_ptr_range(&self) -> Range<*const u8> {
        let start = self.virtual_valid_start().as_ptr().cast_const();
        Range {
            start,
	        // SAFETY: `start` points to the beginning of an allocation which is `self.byte_len` bytes long
            end: unsafe { start.byte_add(self.byte_len()) }
        }
    }

	/// Returns an unsafe mutable to the beginning of the mapping.
	#[must_use]
	pub fn as_mut_ptr(&mut self) -> *mut u8 {
        self.as_mut_ptr_range().start
    }

	/// Returns an unsafe pointer to the beginning of the mapping.
	#[must_use]
	pub fn as_ptr(&self) -> *const u8 {
        self.as_ptr_range().start
    }

	/// Creates a `Mapping` directly from its raw components.
	///
	/// # Safety
	///
	/// A section of virtual memory starting at `base_page` must be allocation by the kernel global virtual allocator.
	/// This section must be equal in length to `R::virtual_size(frames.count())`.
	/// The section of virtual memory from `base_page + R::base_virtual_offset()` must be contiguously mapped to the
	/// physical memory owned by `frames`, with the cache and protection attributes given in `caching` and `protection`.
	///
	/// These requirements are always upheld by the return values of [`into_raw_parts`]. Other mapping sources are
	/// allowed if all the invariants are upheld.
    //#[cfg(not(feature = "use_std"))]
	#[must_use]
	pub unsafe fn from_raw_parts<const RAM: bool, T>(
        frames: Frames<RAM, T>,
        base_page: RawPage,
        protection: Protection,
        caching: Caching,
    ) -> Self where R: Default {
        Self {
            raw: R::default(),
            address_space: ManuallyDrop::new(address_space::Kernel(())),
	        // SAFETY: `from_raw_tuple` called directly on result of `into_raw`
            backing: ManuallyDrop::new(Backing::Contiguous(unsafe { Frames::<false>::from_raw_tuple(Frames::into_raw(frames)) })),
            virtual_start: base_page,
            protection,
            caching,
        }
    }

	/// Decomposes a `Mapping` into its raw components.
    //#[cfg(not(feature = "use_std"))]
    pub fn into_raw_parts(self) -> (Option<Frames<false>>, RawPage, Protection, Caching) {
        let mut this = ManuallyDrop::new(self);
        (
	        // SAFETY: `ManuallyDrop::take` is being called in function consuming `self` so
	        // cannot be called again
            match unsafe { ManuallyDrop::take(&mut this.backing) } {
                Backing::Contiguous(frames) => Some(frames),
                Backing::Discontiguous { .. } | Backing::Vmo { .. } => None,
            },
            this.virtual_start,
            this.protection,
            this.caching,
        )
    }
}

#[cfg(not(feature = "use_std"))]
impl<R: Mappable> Mapping<R, address_space::User> {
	/// Returns the two mutable [user pointers](`crate::ptr#user-pointers`) that span the range of the mapping.
	///
	/// The returned range is half-open, which means that the end pointer points *one past* the last element of the slice.
	/// This way, an empty slice is represented by two equal pointers, and the difference between the two pointers
	/// represents the size of the slice.
    pub fn as_mut_ptr_range(&mut self) -> Range<User<'_, *mut u8>> {
        let start = self.virtual_valid_start().as_ptr();
		// SAFETY: `start` points to the beginning of an allocation which is `self.byte_len` bytes long
        let end = unsafe { start.byte_add(self.byte_len()) };
        let start = User::<*mut u8>::new(start, &self.address_space);
        let end = User::<*mut u8>::new(end, &self.address_space);
        start..end
    }

	/// Returns the two [user pointers](`crate::ptr#user-pointers`) that span the range of the mapping.
	///
	/// The returned range is half-open, which means that the end pointer points *one past* the last element of the slice.
	/// This way, an empty slice is represented by two equal pointers, and the difference between the two pointers
	/// represents the size of the slice.
    pub fn as_ptr_range(&self) -> Range<User<'_, *const u8>> {
        let start = self.virtual_valid_start().as_ptr().cast_const();
		// SAFETY: `start` points to the beginning of an allocation which is `self.byte_len` bytes long
        let end = unsafe { start.byte_add(self.byte_len()) };
		// SAFETY: `start` points to a `u8` which has no invalid bit patterns
        let start = unsafe { User::<*const u8>::new(start, &self.address_space) };
		// SAFETY: `end` points to a `u8` which has no invalid bit patterns
        let end = unsafe { User::<*const u8>::new(end, &self.address_space) };
        start..end
    }

	/// Returns a mutable [user pointer](`crate::ptr#user-pointers`) to the beginning of the mapping.
    pub fn as_mut_ptr(&mut self) -> User<'_, *mut u8> {
        self.as_mut_ptr_range().start
    }

	/// Returns a [user pointer](`crate::ptr#user-pointers`) to the beginning of the mapping.
    pub fn as_ptr(&self) -> User<'_, *const u8> {
        self.as_ptr_range().start
    }
}

impl<R: Mappable, A: address_space::Ty> Mapping<R, A> {
	/// Add `extra_length` pages to the end of the existing mapping.
	///
	/// The mapping is grown in place, such that any addresses pointing within the existing mapping
	/// remain valid.
	///
	/// The underlying physical memory may no longer be contiguous after this operation.
	///
	/// # Errors
	///
	/// Returns an [`AllocError`] describing the allocation that failed.
	///
	/// # Examples
	///
	/// ```
	/// use kernel_api::mapping::{Config, Ty, Mmap};
	/// use kernel_api::memory::PAGE_SIZE;
	/// use core::num::NonZero;
	///
	/// let mut mapping = Config::new(NonZero::new(1).unwrap(), Ty::KERNEL_OTHER)
	///                       .map::<Mmap>()?;
	/// assert_eq!(mapping.page_len(), 1);
	///
	/// mapping.grow_in_place_by(1)?;
	/// assert_eq!(mapping.page_len(), 2);
	/// # Ok::<(), kernel_api::allocator::AllocError>::(())
	/// ```
    pub fn grow_in_place_by(&mut self, extra_length: usize) -> Result<(), AllocError> {
        let Some(extra_length) = NonZero::new(extra_length) else { return Ok(()); };
        let extra_frames = self.backing.pmm().allocate(extra_length)?;

        let extra_pages = self.address_space.allocator()
                                .allocate_contiguous_at(
	                                // FIXME: this only works is `virtual_size()` obeys superposition
                                    self.virtual_start + self.raw.virtual_size(self.page_len()).get(),
                                    extra_length.get(),
                                )?;

        debug!("growing mmap({})", core::any::type_name::<R>());

        match self.address_space.map_contiguous(
            self.virtual_valid_start() + self.page_len(),
            extra_frames.into_raw().0.start,
            extra_length.get(),
            Ty(0),
            self.protection,
            self.caching,
        ) {
            Ok(()) => Ok(()),
            Err(MapPageError::AllocError(err)) => {
                self.address_space.allocator()
                        .deallocate_contiguous(extra_pages, extra_length.get());
                Err(err)
            }
            Err(MapPageError::AlreadyMapped(ty)) => unreachable!("unallocated memory already allocated as {ty:?}"),
        }?;

        let new_backing = Backing::Discontiguous {
            pmm: *self.backing.pmm(),
	        // fixme: provide some kind of warning on overflow instead of just leaking memory?
            frame_count: self.backing.frame_len().saturating_add(extra_length.get()),
        };
        self.backing = ManuallyDrop::new(new_backing); // don't drop the old backing since that could deallocate in-use frames

        Ok(())
    }

	/// The valid length of this mapping in bytes.
	///
	/// # Examples
	///
	/// ```
	/// use kernel_api::mapping::{Config, Ty, Mmap};
	/// use kernel_api::memory::PAGE_SIZE;
	/// use core::num::NonZero;
	///
	/// let mapping = Config::new(NonZero::new(1).unwrap(), Ty::KERNEL_OTHER)
	///                   .map::<Mmap>()?;
	/// assert_eq!(mapping.byte_len(), 1 * PAGE_SIZE);
	/// # Ok::<(), kernel_api::allocator::AllocError>::(())
	/// ```
    pub const fn byte_len(&self) -> usize {
        self.backing.byte_len()
    }

	/// The valid length of this mapping in pages.
	///
	/// # Examples
	///
	/// ```
	/// use kernel_api::mapping::{Config, Ty, Mmap};
	/// use core::num::NonZero;
	///
	/// let mapping = Config::new(NonZero::new(1).unwrap(), Ty::KERNEL_OTHER)
	///                   .map::<Mmap>()?;
	/// assert_eq!(mapping.page_len(), 1);
	/// # Ok::<(), kernel_api::allocator::AllocError>::(())
	/// ```
    pub const fn page_len(&self) -> usize {
        self.backing.frame_len()
    }

	/// Returns the [`RawFrame`] that this mapping is mapped to.
	///
	/// This only makes sense for a physically contiguous mapping, and will return
	/// `None` in other situations.
	///
	/// # Examples
	///
	/// ```
	/// use kernel_api::mapping::{Config, Ty, Mmap};
	/// use core::num::NonZero;
	///
	/// let mapping = Config::new(NonZero::new(1).unwrap(), Ty::KERNEL_OTHER)
	///                   .map::<Mmap>()?;
	/// assert!(mapping.physical_start().is_some());
	/// # Ok::<(), kernel_api::allocator::AllocError>::(())
	/// ```
	///
	/// ```ignore (incomplete)
	/// use kernel_api::mapping::{Config, Ty, Mmap};
	/// use core::num::NonZero;
	///
	/// let mapping = Config::new(NonZero::new(1).unwrap(), Ty::KERNEL_OTHER)
	///                   .with_vmo(get_vmo(), 0)
	///                   .map::<Mmap>()?;
	/// assert!(mapping.physical_start().is_none());
	/// # Ok::<(), kernel_api::allocator::AllocError>::(())
	/// ```
    pub const fn physical_start(&self) -> Option<RawFrame> {
        match &*self.backing {
            Backing::Contiguous(frames) => Some(frames.base()),
            Backing::Discontiguous { .. } | Backing::Vmo { .. } => None,
        }
    }

    /// The first page in the mapping that is mapped to physical memory.
    ///
    /// The region of virtual memory from `virtual_valid_start()` to `virtual_valid_start() + physical_length` is mapped.
    pub fn virtual_valid_start(&self) -> RawPage { self.virtual_start + self.raw.base_virtual_offset() }

	/*
    /// Changes whether a mapping can be written to.
    ///
    /// # Examples
    ///
    /// ```
    /// use kernel_api::mapping::{Config, Ty, Mmap};
    /// use core::num::NonZero;
    ///
    /// let mut mapping = Config::new(NonZero::new(1).unwrap(), Ty::KERNEL_OTHER)
    ///                       .protection(/* writable: */ true, false, false)
    ///                       .map::<Mmap>()?;
    ///
    /// let ptr = mapping.as_mut_ptr();
    /// unsafe { *ptr = 5 };
    ///
    /// mapping.set_writable(false);
    ///
    /// // The following line is now UB
    /// // unsafe { *ptr = 5 };
    ///
    /// // but this is still fine
    /// let val = unsafe { *ptr };
    /// # Ok::<(), kernel_api::allocator::AllocError>::(())
    /// ```
    pub fn set_writable(&mut self, writable: bool) {
        self.protection.writable = writable;
        todo!()
    }*/
}

impl<R: Mappable, A: address_space::Ty> Debug for Mapping<R, A> {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Mapping")
            .field_with(
                "backing",
                |f| {
                    #[cfg(not(feature = "use_std"))] match &*self.backing {
                        Backing::Contiguous(frame) => Debug::fmt(frame, f),
                        Backing::Discontiguous { frame_count, .. } => write!(f, "Discontiguous {{ frame_count: {frame_count} }}"),
                        Backing::Vmo { handle, frame_count } => write!(f, "Vmo {{ handle: {handle:?}, frame_count: {frame_count} }}"),
                    }
                    #[cfg(feature = "use_std")] Ok(())
                }
            )
            .field("address_space", &"<address space>")
            .field("protection", &self.protection)
            .field("caching", &self.caching)
            .finish_non_exhaustive()
    }
}

impl<R: Mappable, A: address_space::Ty> Drop for Mapping<R, A> {
    fn drop(&mut self) {
        info!("drop mmap({})", core::any::type_name::<R>());

	    // SAFETY: `ManuallyDrop::take` called in the `drop` impl so can't be called again
        match unsafe { ManuallyDrop::take(&mut self.backing) } {
            Backing::Contiguous(frames) => drop(frames),
            Backing::Discontiguous { .. } => {
                warn!("ignoring discontiguous page drop");
            }
            Backing::Vmo { handle, .. } => drop(handle),
        }
    }
}