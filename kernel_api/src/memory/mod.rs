//! Provides primitives for interfacing with raw memory (such as [pages](`Page`) and [frames](`Frame`)), as well as
//! interfaces for memory related kernel modules to implement (such as [`BackingAllocator`](allocator::BackingAllocator))
#![stable(feature = "kernel_core_api", since = "0.1.0")]

pub mod allocator;
pub mod heap;
mod type_ops;

const PAGE_SIZE: usize = 4096;
const PAGE_MAP_OFFSET: usize = 0;

/// A memory frame
#[stable(feature = "kernel_core_api", since = "0.1.0")]
#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub struct Frame {
    base: PhysicalAddress<PAGE_SIZE>
}

/// A memory page
#[stable(feature = "kernel_core_api", since = "0.1.0")]
#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub struct Page {
    base: VirtualAddress<PAGE_SIZE>
}

/// A physical memory address of alignment `ALIGN`
#[stable(feature = "kernel_core_api", since = "0.1.0")]
#[derive(Debug, Copy, Clone, Eq, Ord)]
pub struct PhysicalAddress<const ALIGN: usize = 1> {
    #[unstable(feature = "kernel_memory_addr_access", issue = "none")]
    pub addr: usize
}

/// A virtual memory address of alignment `ALIGN`
#[stable(feature = "kernel_core_api", since = "0.1.0")]
#[derive(Debug, Copy, Clone, Eq, Ord)]
pub struct VirtualAddress<const ALIGN: usize = 1> {
    #[unstable(feature = "kernel_memory_addr_access", issue = "none")]
    pub addr: usize
}

impl<const ALIGN: usize> PhysicalAddress<ALIGN> {
    /// Creates a new [`PhysicalAddress`], panicking if the alignment is incorrect
    #[stable(feature = "kernel_core_api", since = "0.1.0")]
    #[rustc_const_stable(feature = "kernel_core_api", since = "0.1.0")]
    #[track_caller]
    pub const fn new(addr: usize) -> Self {
        let unaligned: PhysicalAddress = PhysicalAddress { addr };
        let aligned = unaligned.align_down();

        if aligned.addr != unaligned.addr { panic!("Address not aligned"); }

        aligned
    }

    /// Converts a [`PhysicalAddress`] into a [`VirtualAddress`] via the physical page map region
    #[unstable(feature = "kernel_physical_page_offset", issue = "1")]
    pub const fn to_virtual(self) -> VirtualAddress<ALIGN> {
        VirtualAddress {
            addr: self.addr + PAGE_MAP_OFFSET
        }
    }

    /// Forces a [`PhysicalAddress`] to have a specific alignment
    /// # Safety
    /// The address must be already aligned to the new alignment
    #[stable(feature = "kernel_core_api", since = "0.1.0")]
    #[rustc_const_stable(feature = "kernel_core_api", since = "0.1.0")]
    pub const unsafe fn align_unchecked<const NEW_ALIGN: usize>(self) -> PhysicalAddress<NEW_ALIGN> {
        PhysicalAddress {
            .. self
        }
    }

    /// Returns the [`PhysicalAddress`] less than or equal to `self` with the given alignment
    #[stable(feature = "kernel_core_api", since = "0.1.0")]
    #[rustc_const_stable(feature = "kernel_core_api", since = "0.1.0")]
    pub const fn align_down<const NEW_ALIGN: usize>(self) -> PhysicalAddress<NEW_ALIGN> {
        PhysicalAddress {
            addr: self.addr & !(NEW_ALIGN - 1)
        }
    }

    /// Returns the [`PhysicalAddress`] greater than or equal to `self` with the given alignment
    #[stable(feature = "kernel_core_api", since = "0.1.0")]
    #[rustc_const_stable(feature = "kernel_core_api", since = "0.1.0")]
    pub const fn align_up<const NEW_ALIGN: usize>(self) -> PhysicalAddress<NEW_ALIGN> {
        // FIXME(const): use normal add implementation
        let a: PhysicalAddress = PhysicalAddress {
            addr: self.addr + NEW_ALIGN - 1
        };
        a.align_down()
    }

    #[unstable(feature = "kernel_address_alignment_runtime", issue = "none")]
    pub const fn align_down_runtime(self, new_alignment: usize) -> PhysicalAddress<1> {
        PhysicalAddress {
            addr: self.addr & !(new_alignment - 1)
        }
    }

    #[unstable(feature = "kernel_address_alignment_runtime", issue = "none")]
    pub const fn align_up_runtime(self, new_alignment: usize) -> PhysicalAddress<1> {
        let a: PhysicalAddress = PhysicalAddress {
            addr: self.addr + new_alignment - 1
        };
        a.align_down_runtime(new_alignment)
    }
}

impl Frame {
    /// Converts a [`Frame`] into a [`Page`] via the physical page map region
    #[unstable(feature = "kernel_physical_page_offset", issue = "1")]
    pub const fn to_page(&self) -> Page {
        Page {
            base: self.base.to_virtual()
        }
    }

    #[unstable(feature = "kernel_frame_zero", issue = "none")]
    pub const fn zero() -> Frame {
        Frame::new(PhysicalAddress::new(0))
    }

    /// Creates a [`Frame`] using `base` as the first address within it
    #[stable(feature = "kernel_core_api", since = "0.1.0")]
    #[rustc_const_stable(feature = "kernel_core_api", since = "0.1.0")]
    pub const fn new(base: PhysicalAddress<PAGE_SIZE>) -> Self {
        Self { base }
    }

    #[stable(feature = "kernel_core_api", since = "0.1.0")]
    #[rustc_const_stable(feature = "kernel_core_api", since = "0.1.0")]
    pub const fn checked_sub(&self, rhs: usize) -> Option<Self> {
        // FIXME(const): Option::map
        match self.base.addr.checked_sub(rhs * PAGE_SIZE) {
            Some(addr) => Some(Self {
                base: PhysicalAddress::new(addr)
            }),
            None => None
        }
    }

    /// Returns the first address within the [`Frame`]
    #[stable(feature = "kernel_core_api", since = "0.1.0")]
    #[rustc_const_stable(feature = "kernel_core_api", since = "0.1.0")]
    pub const fn start(&self) -> PhysicalAddress<PAGE_SIZE> {
        self.base
    }

    /// Returns the address one after the end of the [`Frame`]
    #[stable(feature = "kernel_core_api", since = "0.1.0")]
    #[rustc_const_stable(feature = "kernel_core_api", since = "0.1.0")]
    pub const fn end(&self) -> PhysicalAddress<PAGE_SIZE> {
        // FIXME(const): use normal add implementation
        PhysicalAddress::<4096>::new(self.base.addr + PAGE_SIZE)
    }
}

impl<const ALIGN: usize> VirtualAddress<ALIGN> {
    /// Creates a new [`VirtualAddress`], panicking if the alignment is incorrect
    #[stable(feature = "kernel_core_api", since = "0.1.0")]
    #[rustc_const_stable(feature = "kernel_core_api", since = "0.1.0")]
    #[track_caller]
    pub const fn new(addr: usize) -> Self {
        let unaligned: VirtualAddress = VirtualAddress { addr };
        let aligned = unaligned.align_down();

        if aligned.addr != unaligned.addr { panic!("Address not aligned"); }

        aligned
    }

    /// Converts a [`VirtualAddress`] into a raw pointer
    #[stable(feature = "kernel_core_api", since = "0.1.0")]
    #[rustc_const_stable(feature = "kernel_core_api", since = "0.1.0")]
    pub const fn as_ptr(self) -> *mut u8 {
        self.addr as _
    }

    /// Forces a [`VirtualAddress`] to have a specific alignment
    /// # Safety
    /// The address must be already aligned to the new alignment
    #[stable(feature = "kernel_core_api", since = "0.1.0")]
    #[rustc_const_stable(feature = "kernel_core_api", since = "0.1.0")]
    pub const unsafe fn align_unchecked<const NEW_ALIGN: usize>(self) -> VirtualAddress<NEW_ALIGN> {
        VirtualAddress {
            .. self
        }
    }

    /// Returns the [`VirtualAddress`] less than or equal to `self` with the given alignment
    #[stable(feature = "kernel_core_api", since = "0.1.0")]
    #[rustc_const_stable(feature = "kernel_core_api", since = "0.1.0")]
    pub const fn align_down<const NEW_ALIGN: usize>(self) -> VirtualAddress<NEW_ALIGN> {
        VirtualAddress {
            addr: self.addr & !(NEW_ALIGN - 1)
        }
    }

    /// Returns the [`VirtualAddress`] greater than or equal to `self` with the given alignment
    #[stable(feature = "kernel_core_api", since = "0.1.0")]
    #[rustc_const_stable(feature = "kernel_core_api", since = "0.1.0")]
    pub const fn align_up<const NEW_ALIGN: usize>(self) -> VirtualAddress<NEW_ALIGN> {
        // FIXME: const ops
        let a: VirtualAddress = VirtualAddress {
            addr: self.addr + NEW_ALIGN - 1
        };
        a.align_down()
    }

    #[unstable(feature = "kernel_address_alignment_runtime", issue = "none")]
    pub const fn align_down_runtime(self, new_alignment: usize) -> VirtualAddress<1> {
        VirtualAddress {
            addr: self.addr & !(new_alignment - 1)
        }
    }

    #[unstable(feature = "kernel_address_alignment_runtime", issue = "none")]
    pub const fn align_up_runtime(self, new_alignment: usize) -> VirtualAddress<1> {
        let a: VirtualAddress = VirtualAddress {
            addr: self.addr + new_alignment - 1
        };
        a.align_down_runtime(new_alignment)
    }
}

impl Page {
    /// Converts a [`Page`] into a raw pointer pointing to the first address within the page
    #[stable(feature = "kernel_core_api", since = "0.1.0")]
    #[rustc_const_stable(feature = "kernel_core_api", since = "0.1.0")]
    pub const fn as_ptr(&self) -> *mut u8 {
        self.base.as_ptr()
    }

    /// Creates a [`Page`] using `base` as the first address within it
    #[stable(feature = "kernel_core_api", since = "0.1.0")]
    #[rustc_const_stable(feature = "kernel_core_api", since = "0.1.0")]
    pub const fn new(base: VirtualAddress<PAGE_SIZE>) -> Self {
        Self { base }
    }

    /// Returns the first address within the [`Page`]
    #[stable(feature = "kernel_core_api", since = "0.1.0")]
    #[rustc_const_stable(feature = "kernel_core_api", since = "0.1.0")]
    pub const fn start(&self) -> VirtualAddress<PAGE_SIZE> {
        self.base
    }

    /// Returns the address one after the end of the [`Page`]
    #[stable(feature = "kernel_core_api", since = "0.1.0")]
    #[rustc_const_stable(feature = "kernel_core_api", since = "0.1.0")]
    pub const fn end(&self) -> VirtualAddress<PAGE_SIZE> {
        // FIXME(const): use normal add implementation
        VirtualAddress::<4096>::new(self.base.addr + PAGE_SIZE)
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
