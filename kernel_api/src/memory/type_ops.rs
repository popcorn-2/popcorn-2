use core::cmp::{Ord, Ordering, PartialEq, PartialOrd};
use core::iter::Step;
use core::ops::{Add, Sub};

use super::{Frame, Page, PAGE_SIZE, PhysicalAddress, VirtualAddress};

#[stable(feature = "kernel_core_api", since = "0.1.0")]
impl<const A: usize, const B: usize> PartialEq<PhysicalAddress<A>> for PhysicalAddress<B> {
    fn eq(&self, other: &PhysicalAddress<A>) -> bool {
        self.addr == other.addr
    }
}

#[stable(feature = "kernel_core_api", since = "0.1.0")]
impl<const A: usize, const B: usize> PartialEq<VirtualAddress<A>> for VirtualAddress<B> {
    fn eq(&self, other: &VirtualAddress<A>) -> bool {
        self.addr == other.addr
    }
}

#[stable(feature = "kernel_core_api", since = "0.1.0")]
impl<const A: usize, const B: usize> PartialOrd<PhysicalAddress<A>> for PhysicalAddress<B> {
    fn partial_cmp(&self, other: &PhysicalAddress<A>) -> Option<Ordering> {
        self.addr.partial_cmp(&other.addr)
    }
}

#[stable(feature = "kernel_core_api", since = "0.1.0")]
impl<const A: usize, const B: usize> PartialOrd<VirtualAddress<A>> for VirtualAddress<B> {
    fn partial_cmp(&self, other: &VirtualAddress<A>) -> Option<Ordering> {
        self.addr.partial_cmp(&other.addr)
    }
}

#[stable(feature = "kernel_core_api", since = "0.1.0")]
impl<const ALIGN: usize> Add<usize> for VirtualAddress<ALIGN> {
    type Output = VirtualAddress<1>;

    fn add(self, rhs: usize) -> Self::Output {
        VirtualAddress {
            addr: self.addr + rhs
        }
    }
}

#[stable(feature = "kernel_core_api", since = "0.1.0")]
impl<const ALIGN: usize> Add<usize> for PhysicalAddress<ALIGN> {
    type Output = PhysicalAddress<1>;

    fn add(self, rhs: usize) -> Self::Output {
        PhysicalAddress {
            addr: self.addr + rhs
        }
    }
}

#[stable(feature = "kernel_core_api", since = "0.1.0")]
impl<const ALIGN: usize> Add<isize> for VirtualAddress<ALIGN> {
    type Output = VirtualAddress<1>;

    #[track_caller]
    fn add(self, rhs: isize) -> Self::Output {
        #[cfg(debug_assertions)]
        return VirtualAddress {
            addr: self.addr.checked_add_signed(rhs).expect("attempt to add with overflow")
        };

        #[cfg(not(debug_assertions))]
        return VirtualAddress {
            addr: self.addr.wrapping_add_signed(rhs)
        };
    }
}

#[stable(feature = "kernel_core_api", since = "0.1.0")]
impl<const ALIGN: usize> Add<isize> for PhysicalAddress<ALIGN> {
    type Output = PhysicalAddress<1>;

    #[track_caller]
    fn add(self, rhs: isize) -> Self::Output {
        #[cfg(debug_assertions)]
        return PhysicalAddress {
            addr: self.addr.checked_add_signed(rhs).expect("attempt to add with overflow")
        };

        #[cfg(not(debug_assertions))]
        return PhysicalAddress {
            addr: self.addr.wrapping_add_signed(rhs)
        };
    }
}

#[stable(feature = "kernel_core_api", since = "0.1.0")]
impl<const ALIGN: usize> Sub<usize> for VirtualAddress<ALIGN> {
    type Output = VirtualAddress<1>;

    fn sub(self, rhs: usize) -> Self::Output {
        VirtualAddress {
            addr: self.addr - rhs
        }
    }
}

#[stable(feature = "kernel_core_api", since = "0.1.0")]
impl<const ALIGN: usize> Sub<usize> for PhysicalAddress<ALIGN> {
    type Output = PhysicalAddress<1>;

    fn sub(self, rhs: usize) -> Self::Output {
        PhysicalAddress {
            addr: self.addr - rhs
        }
    }
}

#[stable(feature = "kernel_core_api", since = "0.1.0")]
impl<const ALIGN: usize> Sub<isize> for VirtualAddress<ALIGN> {
    type Output = VirtualAddress<1>;

    #[track_caller]
    fn sub(self, rhs: isize) -> Self::Output {
        #[cfg(debug_assertions)]
        return VirtualAddress {
            addr: self.addr.checked_add_signed(-rhs).expect("attempt to subtract with overflow")
        };

        #[cfg(not(debug_assertions))]
        return VirtualAddress {
            addr: self.addr.wrapping_add_signed(-rhs)
        };
    }
}

#[stable(feature = "kernel_core_api", since = "0.1.0")]
impl<const ALIGN: usize> Sub<isize> for PhysicalAddress<ALIGN> {
    type Output = PhysicalAddress<1>;

    #[track_caller]
    fn sub(self, rhs: isize) -> Self::Output {
        #[cfg(debug_assertions)]
        return PhysicalAddress {
            addr: self.addr.checked_add_signed(-rhs).expect("attempt to subtract with overflow")
        };

        #[cfg(not(debug_assertions))]
        return PhysicalAddress {
            addr: self.addr.wrapping_add_signed(-rhs)
        };
    }
}

#[stable(feature = "kernel_core_api", since = "0.1.0")]
impl<const A: usize, const B: usize> Sub<VirtualAddress<A>> for VirtualAddress<B> {
    type Output = usize;

    fn sub(self, rhs: VirtualAddress<A>) -> Self::Output {
        self.addr - rhs.addr
    }
}

#[stable(feature = "kernel_core_api", since = "0.1.0")]
impl<const A: usize, const B: usize> Sub<PhysicalAddress<A>> for PhysicalAddress<B> {
    type Output = usize;

    fn sub(self, rhs: PhysicalAddress<A>) -> Self::Output {
        self.addr - rhs.addr
    }
}

#[stable(feature = "kernel_core_api", since = "0.1.0")]
impl Add<usize> for Page {
    type Output = Page;

    fn add(self, rhs: usize) -> Self::Output {
        Page {
            base: unsafe { (self.base + rhs * PAGE_SIZE).align_unchecked() }
        }
    }
}

#[stable(feature = "kernel_core_api", since = "0.1.0")]
impl Add<usize> for Frame {
    type Output = Frame;

    fn add(self, rhs: usize) -> Self::Output {
        Frame {
            base: unsafe { (self.base + rhs * PAGE_SIZE).align_unchecked() }
        }
    }
}

#[stable(feature = "kernel_core_api", since = "0.1.0")]
impl Add<isize> for Page {
    type Output = Page;

    fn add(self, rhs: isize) -> Self::Output {
        Page {
            base: unsafe { (self.base + rhs * (PAGE_SIZE as isize)).align_unchecked() }
        }
    }
}

#[stable(feature = "kernel_core_api", since = "0.1.0")]
impl Add<isize> for Frame {
    type Output = Frame;

    fn add(self, rhs: isize) -> Self::Output {
        Frame {
            base: unsafe { (self.base + rhs * (PAGE_SIZE as isize)).align_unchecked() }
        }
    }
}

#[stable(feature = "kernel_core_api", since = "0.1.0")]
impl Sub<usize> for Page {
    type Output = Page;

    fn sub(self, rhs: usize) -> Self::Output {
        Page {
            base: unsafe { (self.base - rhs * PAGE_SIZE).align_unchecked() }
        }
    }
}

#[stable(feature = "kernel_core_api", since = "0.1.0")]
impl Sub<usize> for Frame {
    type Output = Frame;

    fn sub(self, rhs: usize) -> Self::Output {
        Frame {
            base: unsafe { (self.base - rhs * PAGE_SIZE).align_unchecked() }
        }
    }
}

#[stable(feature = "kernel_core_api", since = "0.1.0")]
impl Sub<isize> for Page {
    type Output = Page;

    fn sub(self, rhs: isize) -> Self::Output {
        Page {
            base: unsafe { (self.base - rhs * (PAGE_SIZE as isize)).align_unchecked() }
        }
    }
}

#[stable(feature = "kernel_core_api", since = "0.1.0")]
impl Sub<isize> for Frame {
    type Output = Frame;

    fn sub(self, rhs: isize) -> Self::Output {
        Frame {
            base: unsafe { (self.base - rhs * (PAGE_SIZE as isize)).align_unchecked() }
        }
    }
}

#[stable(feature = "kernel_core_api", since = "0.1.0")]
impl Sub<Page> for Page {
    type Output = usize;

    fn sub(self, rhs: Page) -> Self::Output {
        (self.base.addr - rhs.base.addr) / PAGE_SIZE
    }
}

#[stable(feature = "kernel_core_api", since = "0.1.0")]
impl Sub<Frame> for Frame {
    type Output = usize;

    fn sub(self, rhs: Frame) -> Self::Output {
        (self.base.addr - rhs.base.addr) / PAGE_SIZE
    }
}

#[stable(feature = "kernel_core_api", since = "0.1.0")]
impl Step for Frame {
    fn steps_between(start: &Self, end: &Self) -> Option<usize> {
        end.base.addr.checked_sub(start.base.addr)
            .map(|diff| diff / PAGE_SIZE)
    }

    fn forward_checked(start: Self, count: usize) -> Option<Self> {
        let addr_offset = count.checked_mul(PAGE_SIZE)?;
        let base = start.base.addr.checked_add(addr_offset)?;

        Some(Frame {
            base: unsafe { PhysicalAddress::<1>::new(base).align_unchecked() }
        })
    }

    fn backward_checked(start: Self, count: usize) -> Option<Self> {
        let addr_offset = count.checked_mul(PAGE_SIZE)?;
        let base = start.base.addr.checked_sub(addr_offset)?;

        Some(Frame {
            base: unsafe { PhysicalAddress::<1>::new(base).align_unchecked() }
        })
    }
}

#[stable(feature = "kernel_core_api", since = "0.1.0")]
impl Step for Page {
    fn steps_between(start: &Self, end: &Self) -> Option<usize> {
        end.base.addr.checked_sub(start.base.addr)
            .map(|diff| diff / PAGE_SIZE)
    }

    fn forward_checked(start: Self, count: usize) -> Option<Self> {
        let addr_offset = count.checked_mul(PAGE_SIZE)?;
        let base = start.base.addr.checked_add(addr_offset)?;

        Some(Page {
            base: unsafe { VirtualAddress::<1>::new(base).align_unchecked() }
        })
    }

    fn backward_checked(start: Self, count: usize) -> Option<Self> {
        let addr_offset = count.checked_mul(PAGE_SIZE)?;
        let base = start.base.addr.checked_sub(addr_offset)?;

        Some(Page {
            base: unsafe { VirtualAddress::<1>::new(base).align_unchecked() }
        })
    }
}
