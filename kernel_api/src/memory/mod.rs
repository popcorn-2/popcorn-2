//! Provides primitives for interfacing with memory
//! 
//! TODO: memory map overview?
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

use core::cell::UnsafeCell;
use core::fmt::{Debug, Formatter};
use core::ops::Deref;
use core::iter::Step;
use core::fmt;
#[cfg(feature = "full")] use core::slice;
#[cfg(feature = "full")] use core::ops::Range;
#[cfg(feature = "full")] use core::marker::PhantomData;
#[cfg(feature = "full")] use core::mem::{ManuallyDrop, MaybeUninit};
#[cfg(feature = "full")] use crate::allocator::{DynPmm, Pmm};

mod type_ops;

pub mod asan;

/// The number of bytes in the smallest sized page for the current architecture
pub const PAGE_SIZE: usize = const {
    if cfg!(doc) { 0 }
    else if cfg!(target_arch = "x86_64") { 4096 }
    else { panic!("unsupported arch") }
};

const PAGE_MAP_OFFSET: usize = 0xffff_8000_0000_0000;

#[derive(Debug, Copy, Clone, Eq, Ord, Hash, PartialOrd, PartialEq)]
#[must_use = "must be explicitly deallocated to not leak memory"]
pub struct RawPage {
    inner: VirtualAddress,
}

impl RawPage {
    #[track_caller]
    pub fn new(addr: usize) -> Self {
        assert!(addr.is_multiple_of(PAGE_SIZE), "unaligned `RawPage`");
        RawPage { inner: VirtualAddress::new(addr) }
    }
}

impl const Deref for RawPage {
    type Target = VirtualAddress;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

#[derive(Debug, Copy, Clone, Eq, Ord, Hash, PartialOrd, PartialEq)]
#[must_use = "must be explicitly deallocated to not leak memory"]
pub struct RawFrame {
    inner: PhysicalAddress,
}

impl RawFrame {
    #[track_caller]
    pub const fn new(addr: usize) -> Self {
        assert!(addr.is_multiple_of(PAGE_SIZE), "unaligned `RawFrame`");
        RawFrame { inner: PhysicalAddress::new(addr) }
    }
    
    pub const fn checked_sub(self, count: usize) -> Option<Self> {
        self.addr.checked_sub(count * PAGE_SIZE)
                .map(RawFrame::new)
    }
}

impl const Deref for RawFrame {
    type Target = PhysicalAddress;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

/// An owned region of physical memory
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
	pub fn pmm(&self) -> &DynPmm<'static, RAM> { &self.pmm }

    pub fn count(&self) -> usize {
        self.raw.end - self.raw.start
    }
    
    pub fn base(&self) -> RawFrame {
        self.raw.start
    }

	pub fn as_frame_range(&self) -> Range<RawFrame> {
		self.raw.clone()
	}

    /// # Safety
    ///
    /// Behaviour is undefined if any of the following conditions are violated:
    ///
    /// * `self` must be valid for reads.
    /// * `self` must be properly aligned.
    /// * `self` must point to a properly initialized value of `Frames`.
    pub unsafe fn base_raw(self: *const Self) -> RawFrame {
        unsafe { (*self).raw.start }
    }

    pub(crate) fn into_raw(self) -> (Range<RawFrame>, DynPmm<'static, RAM>) {
        let this = ManuallyDrop::new(self);
	    (this.raw.clone(), this.pmm)
    }

    /// # Safety
    ///
    /// `raw` must be an unaliased region of memory allocated by the allocator `pmm`.
    pub unsafe fn from_raw(raw: Range<RawFrame>, pmm: DynPmm<'static, RAM>) -> Self {
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
    pub(crate) unsafe fn from_raw_tuple<const RAM: bool>((raw, pmm): (Range<RawFrame>, DynPmm<'static, RAM>)) -> Self {
		Self {
			raw,
			pmm: pmm.into(),
			_phantom: PhantomData
		}
	}
}

#[cfg(feature = "full")]
impl<const RAM: bool, T> Frames<RAM, MaybeUninit<T>> {
    pub fn cast<U>(self) -> Frames<RAM, MaybeUninit<U>> {
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
    pub fn write_filled(&mut self, value: T) -> &mut [T] where T: Clone {
        self.get_mut().write_filled(value)
    }
    
    pub fn write_filled_with(&mut self, f: impl FnMut(usize) -> T) -> &mut [T] {
        self.get_mut().write_with(f)
    }

    pub fn into_filed(mut self, value: T) -> Frames<true, T> where T: Clone {
        self.write_filled(value);
	    let (raw, pmm) = self.into_raw();
	    Frames {
		    raw,
		    pmm,
		    _phantom: PhantomData
	    }
    }

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
    pub fn get(&self) -> &[T] {
        assert!(align_of::<T>() <= 4096);
        let base = self.raw.start.to_virtual().as_ptr();
        unsafe {
            slice::from_raw_parts(base.cast_const().cast(), self.count() * PAGE_SIZE / size_of::<T>())
        }
    }

    pub fn get_mut(&mut self) -> &mut [T] {
        assert!(align_of::<T>() <= 4096);
        let base = self.raw.start.to_virtual().as_ptr();
        unsafe {
            slice::from_raw_parts_mut(base.cast(), self.count() * PAGE_SIZE / size_of::<T>())
        }
    }
}

#[cfg(feature = "full")]
impl<const RAM: bool, T> Drop for Frames<RAM, T> {
    fn drop(&mut self) {
        unsafe {
	        self.pmm.deallocate_raw(self.raw.start, self.raw.clone().count().try_into().unwrap());
        }
    }
}

/// A physical memory address
#[derive(Debug, Copy, Clone, Eq, Ord, Hash, PartialOrd, PartialEq)]
#[repr(transparent)]
pub struct PhysicalAddress {
    /// The underlying address
    pub addr: usize
}

/// A virtual memory address
#[derive(Debug, Copy, Clone, Eq, Ord, Hash, PartialOrd, PartialEq)]
#[repr(transparent)]
pub struct VirtualAddress {
    /// The underlying address
    pub addr: usize
}

impl PhysicalAddress {
    /// Creates a new [`PhysicalAddress`]
    #[track_caller]
    pub const fn new(addr: usize) -> Self {
        Self { addr }
    }

    /// Converts an [`PhysicalAddress`] into an [`VirtualAddress`] via the physical page map region
    ///
    /// The returned [`VirtualAddress`] is only safe to access if the [`PhysicalAddress`] points into conventional
    /// RAM
    pub const fn to_virtual(self) -> VirtualAddress {
        VirtualAddress::new(self.addr + PAGE_MAP_OFFSET)
    }

    /// Returns the closest [`RawFrame`] at or below the current address
    pub const fn align_down_to_frame(self) -> RawFrame {
        let aligned = PhysicalAddress {
            addr: self.addr & !(PAGE_SIZE - 1)
        };
        RawFrame { inner: aligned }
    }

    /// Returns the closest [`RawFrame`] at or above the current address
    pub const fn align_up_to_frame(self) -> RawFrame {
        let a: PhysicalAddress = self + PAGE_SIZE - 1usize;
        a.align_down_to_frame()
    }

    /// Returns `true` if the [`PhysicalAddress`] is aligned to `align`
    ///
    /// # Panics
    /// 
    /// If `align` is not a power of two
    #[cfg_attr(debug_assertions, track_caller)]
    pub const fn aligned_to(self, align: usize) -> bool {
        #[cfg(debug_assertions)] if !align.is_power_of_two() { panic!("alignment must be power of 2") }
        self.addr & (align - 1) == 0
    }
}

impl VirtualAddress {
    /// Returns `true` if the [`VirtualAddress`] is in the upper half of the address space, i.e. kernelspace
    pub const fn is_higher_half(self) -> bool {
        (self.addr as isize) < 0
    }

    /// Creates a new [`VirtualAddress`]
    #[track_caller]
    pub const fn new(addr: usize) -> Self {
        Self { addr }
    }

    /// Converts a [`VirtualAddress`] into a raw pointer
    #[inline]
    pub const fn as_ptr(self) -> *mut u8 {
        self.addr as _
    }

    /// Returns the closest [`RawPage`] at or below the current address
    pub const fn align_down_to_page(self) -> RawPage {
        let aligned = VirtualAddress {
            addr: self.addr & !(PAGE_SIZE - 1)
        };
        RawPage { inner: aligned }
    }

    /// Returns the closest [`RawPage`] at or above the current address
    pub const fn align_up_to_page(self) -> RawPage {
        let a: VirtualAddress = self + PAGE_SIZE - 1usize;
        a.align_down_to_page()
    }

    /// Returns `true` if the [`PhysicalAddress`] is aligned to `align`
    ///
    /// # Panics
    ///
    /// If `align` is not a power of two
    #[cfg_attr(debug_assertions, track_caller)]
    pub const fn aligned_to(self, align: usize) -> bool {
        #[cfg(debug_assertions)] if !align.is_power_of_two() { panic!("alignment must be power of 2") }
        self.addr & (align - 1) == 0
    }

    /// Computed `self + rhs` saturating when the [`VirtualAddress`] reaches [`usize::MAX`]
    pub const fn saturating_add(self, rhs: usize) -> Self {
        VirtualAddress::new(self.addr.saturating_add(rhs))
    }

    /// Computed `self - rhs` saturating when the [`VirtualAddress`] reaches 0
    pub const fn saturating_sub(self, rhs: usize) -> Self {
        VirtualAddress::new(self.addr.saturating_sub(rhs))
    }

	/// Converts an [`VirtualAddress`] in the physical page map region into an [`PhysicalAddress`]
	///
	/// # Panics
	/// 
	/// Panics if the address is not in the physical page map region on a best effort basis
	pub const fn to_physical(self) -> PhysicalAddress {
		PhysicalAddress::new(self.addr - PAGE_MAP_OFFSET)
	}
}

#[cfg(feature = "full")]
pub struct EpochGuard {
    phantom: PhantomData<UnsafeCell<()>>,
}

#[cfg(feature = "full")]
impl EpochGuard {
    /// # Safety
    ///
    /// This cpu must be in an epoch.
    #[doc(hidden)]
    pub unsafe fn new() -> Self {
        Self { phantom: PhantomData }
    }
}

impl<T: ?Sized> From<*mut T> for VirtualAddress {
    fn from(value: *mut T) -> Self {
        VirtualAddress { addr: value as *mut u8 as usize }
    }
}

impl<T: ?Sized> From<*const T> for VirtualAddress {
    fn from(value: *const T) -> Self {
        VirtualAddress { addr: value as *const u8 as usize }
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
        Some(PhysicalAddress::new(
            Step::forward_checked(start.addr, count)?
        ))
    }

    fn backward_checked(start: Self, count: usize) -> Option<Self> {
        Some(PhysicalAddress::new(
            Step::backward_checked(start.addr, count)?
        ))
    }
}

impl Step for VirtualAddress {
    fn steps_between(start: &Self, end: &Self) -> (usize, Option<usize>) {
        Step::steps_between(&start.addr, &end.addr)
    }

    fn forward_checked(start: Self, count: usize) -> Option<Self> {
        Some(VirtualAddress::new(
            Step::forward_checked(start.addr, count)?
        ))
    }

    fn backward_checked(start: Self, count: usize) -> Option<Self> {
        Some(VirtualAddress::new(
            Step::backward_checked(start.addr, count)?
        ))
    }
}

impl Step for RawFrame {
    fn steps_between(start: &Self, end: &Self) -> (usize, Option<usize>) {
        Step::steps_between(&(start.addr / PAGE_SIZE), &(end.addr / PAGE_SIZE))
    }

    fn forward_checked(start: Self, count: usize) -> Option<Self> {
        Some(RawFrame::new(
            Step::forward_checked(start.addr, count.checked_mul(PAGE_SIZE)?)?
        ))
    }

    fn backward_checked(start: Self, count: usize) -> Option<Self> {
        Some(RawFrame::new(
            Step::backward_checked(start.addr, count.checked_mul(PAGE_SIZE)?)?
        ))
    }
}

impl Step for RawPage {
    fn steps_between(start: &Self, end: &Self) -> (usize, Option<usize>) {
        Step::steps_between(&start.addr, &end.addr)
    }

    fn forward_checked(start: Self, count: usize) -> Option<Self> {
        Some(RawPage::new(
            Step::forward_checked(start.addr, count.checked_mul(PAGE_SIZE)?)?
        ))
    }

    fn backward_checked(start: Self, count: usize) -> Option<Self> {
        Some(RawPage::new(
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn align_down() {
        let unaligned: VirtualAddress = VirtualAddress { addr: 0x1567 };
        let aligned = unaligned.align_down::<4096>();
        assert_eq!(aligned.addr, 0x1000);

        let unaligned: VirtualAddress = VirtualAddress { addr: 0x2000 };
        let aligned = unaligned.align_down::<4096>();
        assert_eq!(aligned.addr, 0x2000);

        let unaligned: PhysicalAddress = PhysicalAddress { addr: 0x1567 };
        let aligned = unaligned.align_down::<4096>();
        assert_eq!(aligned.addr, 0x1000);

        let unaligned: PhysicalAddress = PhysicalAddress { addr: 0x2000 };
        let aligned = unaligned.align_down::<4096>();
        assert_eq!(aligned.addr, 0x2000);
    }

    #[test]
    fn align_up() {
        let unaligned: VirtualAddress = VirtualAddress { addr: 0x1567 };
        let aligned = unaligned.align_up::<4096>();
        assert_eq!(aligned.addr, 0x2000);

        let unaligned: VirtualAddress = VirtualAddress { addr: 0x2000 };
        let aligned = unaligned.align_up::<4096>();
        assert_eq!(aligned.addr, 0x2000);

        let unaligned: PhysicalAddress = PhysicalAddress { addr: 0x1567 };
        let aligned = unaligned.align_up::<4096>();
        assert_eq!(aligned.addr, 0x2000);

        let unaligned: PhysicalAddress = PhysicalAddress { addr: 0x2000 };
        let aligned = unaligned.align_up::<4096>();
        assert_eq!(aligned.addr, 0x2000);
    }
}
