//! Provides primitives for interfacing with memory.
//! 
//! TODO(doc): memory map overview?
//! 
//! # Page map region
//! 
//! All conventional memory (i.e. not MMIO, ACPI firmware data, etc.) is mapped into the "page map
//! region", meaning that the kernel can directly access it without having to create a
//! [`Mapping`](crate::mapping::Mapping) first. This is useful for writing to userspace in a different
//! address space, or for storing allocator metadata.
//! 
//! Physical memory owned through a [`Frames<true>`] can be directly accessed via [`Frames::get()`] and
//! [`Frames::get_mut()`]. Raw [`PhysicalAddress`]es can be converted to a [`VirtualAddress`] in the page
//! map region by calling [`PhysicalAddress::to_virtual`]. **The returned address is only safe to access
//! if the [`PhysicalAddress`] pointed to conventional memory.**
//!
//! # Physical memory
//!
//! TODO(doc).
//!
//! # Virtual memory
//!
//! TODO(doc).
//!
//! # Heap memory
//!
//! TODO(doc).

use core::fmt::{Debug, Formatter};
use core::ops::Deref;
use core::iter::Step;
use core::fmt;
#[cfg(feature = "full")] use core::slice;
#[cfg(feature = "full")] use core::ops::Range;
#[cfg(feature = "full")] use core::marker::PhantomData;
#[cfg(feature = "full")] use core::mem::{ManuallyDrop, MaybeUninit};
#[cfg(feature = "full")] use crate::allocator::{DynPmm, Pmm as _};

mod type_ops;

pub mod asan;

/// The number of bytes in the smallest sized page for the current architecture.
pub const PAGE_SIZE: usize = if PAGE_SIZE_SIGNED >= 0 { PAGE_SIZE_SIGNED.cast_unsigned() } else { panic!("cannot have negative page size") };

/// Signed equivalent to [`PAGE_SIZE`].
pub const PAGE_SIZE_SIGNED: isize = cfg_select! {
	target_arch = "x86_64" => 4096,
};

const PAGE_MAP_OFFSET: usize = 0xffff_8000_0000_0000;

/// A virtual memory page.
#[derive_const(Clone, Eq, Ord, PartialOrd, PartialEq)]
#[derive(Debug, Copy, Hash)]
pub struct RawPage {
    inner: VirtualAddress,
}

impl RawPage {
	/// Creates a new `RawPage` with the given address.
	///
    /// # Panics
    ///
    /// If `addr` is not aligned to a multiple of [`PAGE_SIZE`].
    ///
    /// # Examples
    ///
    /// ```
    /// use kernel_api::memory::{RawPage, PAGE_SIZE};
    ///
    /// let page = RawPage::new(PAGE_SIZE * 5);
    /// ```
    ///
    /// Creating a misaligned page will panic:
    /// ```should_panic
    /// use kernel_api::memory::RawPage;
    ///
    /// let page = RawPage::new(3);
    /// ```
    #[track_caller]
	#[must_use]
    pub const fn new(addr: usize) -> Self {
        assert!(addr.is_multiple_of(PAGE_SIZE), "unaligned `RawPage`");

        Self { inner: VirtualAddress::new(addr) }
    }
}

impl const Deref for RawPage {
    type Target = VirtualAddress;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

/// A physical memory frame.
#[derive_const(Clone, Eq, Ord, PartialOrd, PartialEq)]
#[derive(Debug, Copy, Hash)]
pub struct RawFrame {
    inner: PhysicalAddress,
}

impl RawFrame {
	/// Creates a new `RawFrame` with the given address.
	///
	/// # Panics
    ///
    /// If `addr` is not aligned to a multiple of [`PAGE_SIZE`].
    ///
    /// # Examples
    ///
    /// ```
    /// use kernel_api::memory::{RawFrame, PAGE_SIZE};
    ///
    /// let frame = RawFrame::new(PAGE_SIZE * 5);
    /// ```
    ///
    /// Creating a misaligned frame will panic:
    /// ```should_panic
    /// use kernel_api::memory::RawFrame;
    ///
    /// let frame = RawFrame::new(3);
    /// ```
    #[track_caller]
	#[must_use]
    pub const fn new(addr: usize) -> Self {
        assert!(addr.is_multiple_of(PAGE_SIZE), "unaligned `RawFrame`");

        Self { inner: PhysicalAddress::new(addr) }
    }

	/// Checked integer subtraction.
    ///
    /// Computes the frame `count` frames below `self`, returning
	/// `None` if overflow occurred.
    ///
    /// # Examples
    ///
    /// ```
    /// use kernel_api::memory::{RawFrame, PAGE_SIZE};
    ///
    /// let frame = RawFrame::new(PAGE_SIZE * 2);
    /// assert_eq!(frame.checked_sub(1), Some(RawFrame::new(PAGE_SIZE)));
    /// assert_eq!(frame.checked_sub(5), None);
    /// ```
	#[must_use = "this returns the result of the operation, without modifying the original"]
    pub const fn checked_sub(self, count: usize) -> Option<Self> {
        self.addr.checked_sub(count * PAGE_SIZE)
                .map(Self::new)
    }
}

impl const Deref for RawFrame {
    type Target = PhysicalAddress;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

/// An owned region of physical memory.
///
/// If the region is allocated from conventional memory, then the `RAM` parameter is set,
/// and the allocated memory can be directly accessed through [`get`](`Frames::get`) and
/// [`get_mut`](`Frames::get_mut`) via the [page map region](`self#page-map-region`).
///
/// # Examples
///
/// ```
/// use std::num::NonZero;
/// use kernel_api::allocator::highmem;
/// use kernel_api::memory::Frames;
///
/// // allocate some memory from `highmem`
/// let mut mem = highmem().allocate_one()?;
///
/// // initialise the memory with zeroes
/// let slice = mem.write_filled(0);
///
/// // once initialised, the memory can be accessed as a slice
/// slice[1] = 42;
/// info!("{}", slice[0]);
///
/// // the memory is deallocated when dropped
/// drop(mem);
/// # Ok::<(), kernel_api::allocator::AllocError>::(())
/// ```
// INVARIANT: `raw` must be an unaliased range of frames allocated by `pmm`
#[cfg(feature = "full")]
#[repr(C)]
pub struct Frames<const RAM: bool, T = MaybeUninit<u8>> {
    raw: Range<RawFrame>,
	pmm: DynPmm<'static, RAM>,
    _phantom: PhantomData<[T]>,
}

#[cfg(feature = "full")]
impl<const RAM: bool, T> Debug for Frames<RAM, T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("Frames")
                .field("raw", &self.raw)
                .field("pmm", &self.pmm)
                .finish()
    }
}

#[cfg(feature = "full")]
impl<const RAM: bool, T> Frames<RAM, T> {
	/// Returns the physical memory allocator used for the underlying allocation.
    ///
    /// # Examples
    ///
    /// ```
    /// use kernel_api::allocator::highmem;
    ///
    /// let mem = highmem().allocate_one()?;
    /// let allocator = mem.pmm(); // this is a reference to the `highmem` allocator
    /// # Ok::<(), kernel_api::allocator::AllocError>::(())
    /// ```
    #[must_use]
	pub const fn pmm(&self) -> &DynPmm<'static, RAM> { &self.pmm }

	/// Returns the number of allocated frames.
    ///
    /// # Examples
    ///
    /// ```
    /// use kernel_api::allocator::highmem;
    ///
    /// let mem = highmem().allocate_one()?;
    /// assert_eq!(mem.count(), 1);
    /// # Ok::<(), kernel_api::allocator::AllocError>::(())
    /// ```
    #[must_use]
    pub const fn count(&self) -> usize {
        self.raw.end - self.raw.start
    }

	/// Returns the first frame of the allocation.
    ///
    /// # Examples
    ///
    /// ```
    /// use kernel_api::allocator::highmem;
    ///
    /// let mem = highmem().allocate_one()?;
    /// info!("allocation starts at {:?}", mem.base());
    /// # Ok::<(), kernel_api::allocator::AllocError>::(())
    /// ```
    #[must_use]
    pub const fn base(&self) -> RawFrame {
        self.raw.start
    }

	/// Returns the two [`RawFrame`]s that span the allocation.
	///
	/// The returned range is half-open, which means that the end frame points *one past* the last frame of the allocation.
	/// This way, a zero sized allocation is represented by two equal frames, and the difference between the two frames
	/// represents the number of frames of the allocation.
    ///
    /// # Examples
    ///
    /// ```
    /// use kernel_api::allocator::highmem;
    /// use core::ops::Range;
    ///
    /// let mem = highmem().allocate_one()?;
    /// let Range { start, end } = mem.as_frame_range();
    ///
    /// info!("allocation starts at {start:?}");
    /// assert_eq!(start + 1, end);
    /// # Ok::<(), kernel_api::allocator::AllocError>::(())
    /// ```
    #[must_use]
	pub const fn as_frame_range(&self) -> Range<RawFrame> {
		self.raw.clone()
	}

	/// Equivalent to [`base()`](`Self::base`) but taking a raw pointer for `self`.
	///
	/// # Safety
	///
	/// Behavior is undefined if any of the following conditions are violated:
	///
	/// - `self` must be valid for reads.
	/// - `self` must be properly aligned.
	/// - `self` must point to a properly initialized value of `Frames`.
    ///
    /// # Examples
    ///
    /// See examples for [`base()`](Self::base).
    #[expect(rustdoc::missing_doc_code_examples, reason = "example reference to other function")]
    #[must_use]
    pub const unsafe fn base_raw(self: *const Self) -> RawFrame {
		// SAFETY: Safety requirements upheld by caller
        unsafe { (*self).raw.start }
    }

    #[must_use]
    pub(crate) const fn into_raw(self) -> (Range<RawFrame>, DynPmm<'static, RAM>) {
        let this = ManuallyDrop::new(self);
	    (this.raw.clone(), this.pmm)
    }

	/// Creates a `Frames` directly from its raw components.
	///
	/// # Safety
	///
	/// `raw` must be an unaliased region of memory allocated by the allocator `pmm`.
    ///
    /// # Examples
    ///
    /// ```
    /// use kernel_api::allocator::highmem;
    /// use kernel_api::memory::Frames;
    ///
    /// // manually allocate some memory
    /// let allocator = highmem();
    /// let start = allocator.allocate_one_raw()?;
    ///
    /// // wrap it in `Frames` so it deallocates on drop
    /// let frames = unsafe {
    ///     Frames::from_raw(
    ///         start..(start + 1),
    ///         allocator,
    ///     )
    /// };
    /// # Ok::<(), kernel_api::allocator::AllocError>::(())
    /// ```
    #[must_use]
    pub const unsafe fn from_raw(raw: Range<RawFrame>, pmm: DynPmm<'static, RAM>) -> Self {
        Self {
            raw,
	        pmm,
            _phantom: PhantomData
        }
    }
}

#[cfg(feature = "full")]
impl<T> Frames<false, T> {
	/// # Safety
	///
	/// See [`Frames::from_raw`].
	pub(crate) const unsafe fn from_raw_tuple<const RAM: bool>((raw, pmm): (Range<RawFrame>, DynPmm<'static, RAM>)) -> Self {
		Self {
			raw,
			pmm: pmm.into(),
			_phantom: PhantomData
		}
	}
}

#[cfg(feature = "full")]
impl<const RAM: bool, T> Frames<RAM, MaybeUninit<T>> {
	/// Casts the underlying type pointed to by the `Frames`.
    ///
    /// This allows using a `Frames` object directly as a fixed size array for storing data with a
    /// lower alignment requirement than a page.
    ///
    /// # Examples
    ///
    /// ```
    /// use kernel_api::allocator::highmem;
    ///
    /// let mem = highmem().allocate_one()?;
    /// let mem = mem.cast::<u32>();
    /// // `mem` can now be used as a `[u32]`
    /// # Ok::<(), kernel_api::allocator::AllocError>::(())
    /// ```
	#[must_use]
    pub const fn cast<U>(self) -> Frames<RAM, MaybeUninit<U>> {
        let (raw, pmm) = self.into_raw();
	    Frames {
		    raw,
		    pmm,
		    _phantom: PhantomData
	    }
    }
}

#[cfg(feature = "full")]
impl<T> Frames<true, MaybeUninit<T>> {
	/// Fills the memory region with elements by cloning `value`, returning a mutable reference to the now
	/// initialized contents of memory.
	/// Any previously initialized elements will not be dropped.
	///
	/// This is similar to [`slice::fill`].
	///
	/// # Panics
	///
	/// This function will panic if any call to `Clone` panics.
	///
	/// If such a panic occurs, any elements previously initialized during this operation will be
	/// dropped.
    ///
    /// # Examples
    ///
    /// ```
    /// use kernel_api::allocator::highmem;
    /// use kernel_api::memory::PAGE_SIZE;
    ///
    /// let mem = highmem().allocate_one()?;
    /// let initialized = mem.write_filled(1);
    /// assert_eq!(initialized, &[1; PAGE_SIZE]);
    /// # Ok::<(), kernel_api::allocator::AllocError>::(())
    /// ```
    pub fn write_filled(&mut self, value: T) -> &mut [T] where T: Clone {
        self.get_mut().write_filled(value)
    }

	/// Fills the memory region with elements returned by calling a closure for each index, returning a mutable
	/// reference to the now initialized contents of memory.
	/// Any previously initialized elements will not be dropped.
	///
	/// This method uses a closure to create new values. If you'd rather `Clone` a given value, use
	/// [`write_filled`](`Frames::write_filled`). If you want to use the `Default` trait to generate values, you can
	/// pass [`|_| Default::default()`][Default::default] as the argument.
	///
	/// # Panics
	///
	/// This function will panic if any call to the provided closure panics.
	///
	/// If such a panic occurs, any elements previously initialized during this operation will be
	/// dropped.
    ///
    /// # Examples
    ///
    /// ```
    /// use kernel_api::allocator::highmem;
    /// use kernel_api::memory::PAGE_SIZE;
    ///
    /// let mem = highmem().allocate_one()?;
    /// let initialized = mem.write_with(|idx| 1u8.wrapping_add(idx as u8));
    /// assert_eq!(&initialized[..5], &[1, 2, 3, 4, 5]);
    /// # Ok::<(), kernel_api::allocator::AllocError>::(())
    /// ```
    pub fn write_filled_with(&mut self, f: impl FnMut(usize) -> T) -> &mut [T] {
        self.get_mut().write_with(f)
    }

	/// Fills the memory region with elements by cloning `value`, consuming the `Frames`
	/// and returning a new object of the initialized type.
	/// Any previously initialized elements will not be dropped.
	///
	/// This is similar to [`slice::fill`].
	///
	/// # Panics
	///
	/// This function will panic if any call to `Clone` panics.
	///
	/// If such a panic occurs, any elements previously initialized during this operation will be
	/// dropped.
    ///
    /// # Examples
    ///
    /// ```
    /// use kernel_api::allocator::highmem;
    /// use kernel_api::memory::PAGE_SIZE;
    ///
    /// let mem = highmem().allocate_one()?;
    /// let mem = mem.into_filled(1);
    /// let initialized = mem.get();
    /// assert_eq!(initialized, &[1; PAGE_SIZE]);
    /// # Ok::<(), kernel_api::allocator::AllocError>::(())
    /// ```
    pub fn into_filed(mut self, value: T) -> Frames<true, T> where T: Clone {
        self.write_filled(value);
	    let (raw, pmm) = self.into_raw();
	    Frames {
		    raw,
		    pmm,
		    _phantom: PhantomData
	    }
    }

	/// Fills the memory region with elements returned by calling a closure for each index, consuming the `Frames`
	/// and returning a new object of the initialized type.
	/// Any previously initialized elements will not be dropped.
	///
	/// This method uses a closure to create new values. If you'd rather `Clone` a given value, use
	/// [`write_filled`](`Frames::write_filled`). If you want to use the `Default` trait to generate values, you can
	/// pass [`|_| Default::default()`][Default::default] as the argument.
	///
	/// # Panics
	///
	/// This function will panic if any call to the provided closure panics.
	///
	/// If such a panic occurs, any elements previously initialized during this operation will be
	/// dropped.
    ///
    /// # Examples
    ///
    /// ```
    /// use kernel_api::allocator::highmem;
    /// use kernel_api::memory::PAGE_SIZE;
    ///
    /// let mem = highmem().allocate_one()?;
    /// let mem = mem.into_filled_with(|idx| 1u8.wrapping_add(idx as u8));
    /// let initialized = mem.get();
    /// assert_eq!(&initialized[..5], &[1, 2, 3, 4, 5]);
    /// # Ok::<(), kernel_api::allocator::AllocError>::(())
    /// ```
    pub fn into_filed_with(mut self, f: impl FnMut(usize) -> T) -> Frames<true, T> {
        self.write_filled_with(f);
	    let (raw, pmm) = self.into_raw();
	    Frames {
		    raw,
		    pmm,
		    _phantom: PhantomData
	    }
    }
}

#[cfg(feature = "full")]
impl<T> Frames<true, T> {
	/// Returns a slice to the underlying memory.
    ///
    /// This will fail to compile if `T` is a type with higher alignment requirements than a page
    /// guarantees.
    ///
    /// # Examples
    ///
    /// ```
    /// use core::mem::MaybeUninit;
    /// use kernel_api::allocator::highmem;
    ///
    /// let mem = highmem().allocate_one()?;
    /// let buffer: &[MaybeUninit<u8>] = mem.get();
    /// # Ok::<(), kernel_api::allocator::AllocError>::(())
    /// ```
    ///
    /// ```compile_fail
    /// use kernel_api::allocator::highmem;
    ///
    /// #[repr(align(8192))]
    /// struct HighAlignment(u64);
    ///
    /// let mem = highmem().allocate_one()?
    /// let mem = mem.cast::<HighAlignment>();
    /// mem.get();
    /// # Ok::<(), kernel_api::allocator::AllocError>::(())
    /// ```
	#[must_use]
	pub const fn get(&self) -> &[T] {
        const { assert!(align_of::<T>() <= PAGE_SIZE, "Accessing a page as a type requires pages to be aligned enough for the type") };
        let base = self.raw.start.to_virtual().as_ptr();
		// SAFETY: the backing memory must either be initialized to get a `Frames<T>`, or `T = MaybeUninit`,
		//  alignment is checked above, and the pointer always points to a valid place in the page map
		//  region since `RAM = true`
        unsafe {
            slice::from_raw_parts(base.cast_const().cast(), self.count() * PAGE_SIZE / size_of::<T>())
        }
    }

	/// Returns a mutable slice to the underlying memory.
    ///
    /// This will fail to compile if `T` is a type with higher alignment requirements than a page
    /// guarantees.
    ///
    /// # Examples
    ///
    /// ```
    /// use core::mem::MaybeUninit;
    /// use kernel_api::allocator::highmem;
    ///
    /// let mem = highmem().allocate_one()?;
    /// let buffer: &mut [MaybeUninit<u8>] = mem.get_mut();
    /// # Ok::<(), kernel_api::allocator::AllocError>::(())
    /// ```
    ///
    /// ```compile_fail
    /// use kernel_api::allocator::highmem;
    ///
    /// #[repr(align(8192))]
    /// struct HighAlignment(u64);
    ///
    /// let mem = highmem().allocate_one()?
    /// let mut mem = mem.cast::<HighAlignment>();
    /// mem.get_mut();
    /// # Ok::<(), kernel_api::allocator::AllocError>::(())
    /// ```
	#[must_use]
	pub const fn get_mut(&mut self) -> &mut [T] {
        const { assert!(align_of::<T>() <= PAGE_SIZE, "Accessing a page as a type requires pages to be aligned enough for the type") };
        let base = self.raw.start.to_virtual().as_ptr();
		// SAFETY: the backing memory must either be initialized to get a `Frames<T>`, or `T = MaybeUninit`,
		//  alignment is checked above, the pointer always points to a valid place in the page map
		//  region since `RAM = true`, and the memory is uniquely owned by the `Frames` object and so can't
		//  be aliased
        unsafe {
            slice::from_raw_parts_mut(base.cast(), self.count() * PAGE_SIZE / size_of::<T>())
        }
    }
}

#[cfg(feature = "full")]
impl<const RAM: bool, T> Drop for Frames<RAM, T> {
    fn drop(&mut self) {
	    // SAFETY: invariant of `Frames` that frames are allocated by `self.pmm`
        unsafe {
	        self.pmm.deallocate_raw(self.raw.start, self.raw.clone().count().try_into().unwrap());
        }
    }
}

/// A physical memory address.
#[derive_const(Clone, Eq, Ord, PartialOrd, PartialEq)]
#[derive(Debug, Copy, Hash)]
#[repr(transparent)]
pub struct PhysicalAddress {
    #[doc(hidden)]
    pub addr: usize
}

/// A virtual memory address.
#[derive_const(Clone, Eq, Ord, PartialOrd, PartialEq)]
#[derive(Debug, Copy, Hash)]
#[repr(transparent)]
pub struct VirtualAddress {
    #[doc(hidden)]
    pub addr: usize
}

impl PhysicalAddress {
    /// Creates a new `PhysicalAddress` with the given address.
    ///
    /// # Examples
    ///
    /// ```
    /// use kernel_api::memory::PhysicalAddress;
    ///
    /// let addr = PhysicalAddress::new(0);
    /// ```
    #[track_caller]
    #[must_use]
    pub const fn new(addr: usize) -> Self {
        Self { addr }
    }

    /// Converts a `PhysicalAddress` into a [`VirtualAddress`] via the physical page map region.
    ///
    /// The returned `VirtualAddress` is only safe to access if the `PhysicalAddress` points into conventional
    /// RAM.
    ///
    /// This is the physical-to-virtual counterpart to [`to_physical()`](VirtualAddress::to_physical).
    ///
    /// # Examples
    ///
    /// ```
    /// use kernel_api::memory::PhysicalAddress;
    ///
    /// let phys = PhysicalAddress::new(0x1000);
    /// let virt = phys.to_virtual();
    /// // if there was conventional RAM at addr 0x1000,
    /// // `virt` could now be accessed to write to that memory
    /// ```
    #[must_use = "this returns the result of the operation, without modifying the original"]
    pub const fn to_virtual(self) -> VirtualAddress {
        VirtualAddress::new(self.addr + PAGE_MAP_OFFSET)
    }

    /// Returns the closest [`RawFrame`] at or below the current address.
    ///
    /// # Examples
    ///
    /// ```
    /// use kernel_api::memory::{PhysicalAddress, PAGE_SIZE};
    ///
    /// let address = PhysicalAddress::new(0x1);
    /// let frame = address.align_down_to_frame();
    /// assert!(address >= *frame);
    /// assert!((*frame).is_aligned_to(PAGE_SIZE));
    ///
    /// // If the address is already aligned, then the value will remain unchanged.
    /// let address = PhysicalAddress::new(PAGE_SIZE);
    /// let frame = address.align_down_to_frame();
    /// assert!(address == *frame);
    /// ```
    #[must_use = "this returns the result of the operation, without modifying the original"]
    pub const fn align_down_to_frame(self) -> RawFrame {
        let aligned = Self {
            addr: self.addr & !(PAGE_SIZE - 1)
        };
        RawFrame { inner: aligned }
    }

    /// Returns the closest [`RawFrame`] at or above the current address.
    ///
    /// # Examples
    ///
    /// ```
    /// use kernel_api::memory::{PhysicalAddress, PAGE_SIZE};
    ///
    /// let address = PhysicalAddress::new(0x1);
    /// let frame = address.align_up_to_frame();
    /// assert!(address <= *frame);
    /// assert!((*frame).is_aligned_to(PAGE_SIZE));
    ///
    /// // If the address is already aligned, then the value will remain unchanged.
    /// let address = PhysicalAddress::new(PAGE_SIZE);
    /// let frame = address.align_up_to_frame();
    /// assert!(address == *frame);
    /// ```
    #[must_use = "this returns the result of the operation, without modifying the original"]
    pub const fn align_up_to_frame(self) -> RawFrame {
        let addr: Self = self + PAGE_SIZE - 1usize;
        addr.align_down_to_frame()
    }

    /// Returns `true` if the address is aligned to `align`.
    ///
    /// # Panics
    /// 
    /// If `align` is not a power of two.
    ///
    /// # Examples
    ///
    /// ```
    /// use kernel_api::memory::PhysicalAddress;
    ///
    /// let addr = PhysicalAddress::new(64);
    /// assert_eq!(addr.is_aligned_to(32), true);
    /// assert_eq!(addr.is_aligned_to(128), false);
    /// ```
    #[cfg_attr(debug_assertions, track_caller)]
    #[must_use]
    pub const fn is_aligned_to(self, align: usize) -> bool {
        #[cfg(debug_assertions)] assert!(align.is_power_of_two(), "alignment must be power of 2");

        self.addr & (align - 1) == 0
    }
}

impl VirtualAddress {
    /// Creates a new `VirtualAddress` with the given address.
    ///
    /// # Examples
    ///
    /// ```
    /// use kernel_api::memory::VirtualAddress;
    ///
    /// let addr = VirtualAddress::new(0);
    /// ```
    #[track_caller]
    #[must_use]
    pub const fn new(addr: usize) -> Self {
        Self { addr }
    }

    /// Converts a `VirtualAddress` into a raw pointer.
    ///
    /// # Examples
    ///
    /// ```
    /// use kernel_api::memory::VirtualAddress;
    ///
    /// let addr = VirtualAddress::new(0x1000);
    /// let ptr = addr.as_ptr();
    /// // if `addr` came from allocated memory, it can be accessed through `ptr`
    /// // unsafe { *ptr = 5 };
    /// ```
    #[inline]
    #[must_use]
    pub const fn as_ptr(self) -> *mut u8 {
	    core::ptr::with_exposed_provenance_mut(self.addr)
    }

    /// Returns `true` if the address is in the upper half of the address space, i.e. [kernelspace](crate::address_space#kernel-address-space).
    ///
    /// # Examples
    ///
    /// ```
    /// use kernel_api::memory::VirtualAddress;
    ///
    /// let addr = VirtualAddress::new(usize::MAX);
    /// assert_eq!(addr.is_higher_half(), true);
    /// ```
    #[must_use]
    pub const fn is_higher_half(self) -> bool {
        self.addr.cast_signed() < 0
    }

    /// Returns the closest [`RawPage`] at or below the current address.
    ///
    /// # Examples
    ///
    /// ```
    /// use kernel_api::memory::{VirtualAddress, PAGE_SIZE};
    ///
    /// let address = VirtualAddress::new(0x1);
    /// let page = address.align_down_to_page();
    /// assert!(address >= *page);
    /// assert!((*page).is_aligned_to(PAGE_SIZE));
    ///
    /// // If the address is already aligned, then the value will remain unchanged.
    /// let address = VirtualAddress::new(PAGE_SIZE);
    /// let page = address.align_down_to_page();
    /// assert!(address == *page);
    /// ```
    #[must_use = "this returns the result of the operation, without modifying the original"]
    pub const fn align_down_to_page(self) -> RawPage {
        let aligned = Self {
            addr: self.addr & !(PAGE_SIZE - 1)
        };
        RawPage { inner: aligned }
    }

    /// Returns the closest [`RawPage`] at or above the current address.
    ///
    /// # Examples
    ///
    /// ```
    /// use kernel_api::memory::{VirtualAddress, PAGE_SIZE};
    ///
    /// let address = VirtualAddress::new(0x1);
    /// let page = address.align_up_to_page();
    /// assert!(address <= *page);
    /// assert!((*page).is_aligned_to(PAGE_SIZE));
    ///
    /// // If the address is already aligned, then the value will remain unchanged.
    /// let address = VirtualAddress::new(PAGE_SIZE);
    /// let page = address.align_up_to_page();
    /// assert!(address == *page);
    /// ```
    #[must_use = "this returns the result of the operation, without modifying the original"]
    pub const fn align_up_to_page(self) -> RawPage {
        let addr: Self = self + PAGE_SIZE - 1usize;
        addr.align_down_to_page()
    }

    /// Returns `true` if the address is aligned to `align`.
    ///
    /// # Panics
    ///
    /// If `align` is not a power of two.
    ///
    /// # Examples
    ///
    /// ```
    /// use kernel_api::memory::VirtualAddress;
    ///
    /// let addr = VirtualAddress::new(64);
    /// assert_eq!(addr.is_aligned_to(32), true);
    /// assert_eq!(addr.is_aligned_to(128), false);
    /// ```
    #[cfg_attr(debug_assertions, track_caller)]
    #[must_use]
    pub const fn is_aligned_to(self, align: usize) -> bool {
        #[cfg(debug_assertions)] assert!(align.is_power_of_two(), "alignment must be power of 2");

        self.addr & (align - 1) == 0
    }

    /// Computes `self + rhs`, saturating when the address reaches [`usize::MAX`].
    ///
    /// # Examples
    ///
    /// ```
    /// use kernel_api::memory::VirtualAddress;
    ///
    /// let a = VirtualAddress::new(0x1000);
    /// assert_eq!(a.saturating_add(1), Some(VirtualAddress::new(0x1001)));
    /// assert_eq!(a.saturating_add(usize::MAX), None);
    /// ```
    #[must_use = "this returns the result of the operation, without modifying the original"]
    pub const fn saturating_add(self, rhs: usize) -> Self {
        Self::new(self.addr.saturating_add(rhs))
    }

    /// Computes `self - rhs` saturating when the address reaches 0.
    ///
    /// # Examples
    ///
    /// ```
    /// use kernel_api::memory::VirtualAddress;
    ///
    /// let a = VirtualAddress::new(0x1001);
    /// assert_eq!(a.saturating_sub(1), Some(VirtualAddress::new(0x1000)));
    /// assert_eq!(a.saturating_sub(usize::MAX), None);
    /// ```
    #[must_use = "this returns the result of the operation, without modifying the original"]
    pub const fn saturating_sub(self, rhs: usize) -> Self {
        Self::new(self.addr.saturating_sub(rhs))
    }

	/// Converts an address in the physical page map region into an [`PhysicalAddress`].
    ///
    /// This is the virtual-to-physical counterpart to [`to_virtual()`](PhysicalAddress::to_virtual).
	///
	/// # Panics
	/// 
	/// Panics if the address is not in the physical page map region on a best effort basis.
    ///
    /// # Examples
    ///
    /// ```should_panic
    /// use kernel_api::memory::VirtualAddress;
    ///
    /// let virt = VirtualAddress::new(0);
    ///
    /// // converting non physical page map addresses may panic
    /// let phys = virt.to_physical();
    /// ```
	#[must_use = "this returns the result of the operation, without modifying the original"]
	pub const fn to_physical(self) -> PhysicalAddress {
		PhysicalAddress::new(self.addr - PAGE_MAP_OFFSET)
	}
}

impl<T: ?Sized> From<*mut T> for VirtualAddress {
    fn from(value: *mut T) -> Self {
        Self { addr: value.expose_provenance() }
    }
}

impl<T: ?Sized> From<*const T> for VirtualAddress {
    fn from(value: *const T) -> Self {
        Self { addr: value.expose_provenance() }
    }
}

impl const From<RawFrame> for PhysicalAddress {
    fn from(value: RawFrame) -> Self {
        value.inner
    }
}

impl const From<RawPage> for VirtualAddress {
    fn from(value: RawPage) -> Self {
        value.inner
    }
}

impl Step for PhysicalAddress {
    fn steps_between(start: &Self, end: &Self) -> (usize, Option<usize>) {
        Step::steps_between(&start.addr, &end.addr)
    }

    fn forward_checked(start: Self, count: usize) -> Option<Self> {
        Step::forward_checked(start.addr, count).map(Self::new)
    }

    fn backward_checked(start: Self, count: usize) -> Option<Self> {
        Step::backward_checked(start.addr, count).map(Self::new)
    }
}

impl Step for VirtualAddress {
    fn steps_between(start: &Self, end: &Self) -> (usize, Option<usize>) {
        Step::steps_between(&start.addr, &end.addr)
    }

    fn forward_checked(start: Self, count: usize) -> Option<Self> {
        Step::forward_checked(start.addr, count).map(Self::new)
    }

    fn backward_checked(start: Self, count: usize) -> Option<Self> {
        Step::backward_checked(start.addr, count).map(Self::new)
    }
}

impl Step for RawFrame {
    fn steps_between(start: &Self, end: &Self) -> (usize, Option<usize>) {
        Step::steps_between(&(start.addr / PAGE_SIZE), &(end.addr / PAGE_SIZE))
    }

    fn forward_checked(start: Self, count: usize) -> Option<Self> {
        Some(Self::new(
            Step::forward_checked(start.addr, count.checked_mul(PAGE_SIZE)?)?
        ))
    }

    fn backward_checked(start: Self, count: usize) -> Option<Self> {
        Some(Self::new(
            Step::backward_checked(start.addr, count.checked_mul(PAGE_SIZE)?)?
        ))
    }
}

impl Step for RawPage {
    fn steps_between(start: &Self, end: &Self) -> (usize, Option<usize>) {
        Step::steps_between(&start.addr, &end.addr)
    }

    fn forward_checked(start: Self, count: usize) -> Option<Self> {
        Some(Self::new(
            Step::forward_checked(start.addr, count.checked_mul(PAGE_SIZE)?)?
        ))
    }

    fn backward_checked(start: Self, count: usize) -> Option<Self> {
        Some(Self::new(
            Step::backward_checked(start.addr, count.checked_mul(PAGE_SIZE)?)?
        ))
    }
}

impl fmt::LowerHex for VirtualAddress {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        fmt::LowerHex::fmt(&self.addr, f)
    }
}

impl fmt::LowerHex for PhysicalAddress {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        fmt::LowerHex::fmt(&self.addr, f)
    }
}

impl fmt::LowerHex for RawPage {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        fmt::LowerHex::fmt(&self.addr, f)
    }
}

impl fmt::LowerHex for RawFrame {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        fmt::LowerHex::fmt(&self.addr, f)
    }
}
