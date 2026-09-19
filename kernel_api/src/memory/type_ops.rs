use core::ops::{Add, Sub};

use super::{PAGE_SIZE, PhysicalAddress, RawFrame, RawPage, VirtualAddress};

const impl Add<usize> for VirtualAddress {
    type Output = Self;

    #[track_caller]
    fn add(self, rhs: usize) -> Self::Output {
        Self::new(self.addr + rhs)
    }
}

const impl Add<usize> for PhysicalAddress {
    type Output = Self;

    #[track_caller]
    fn add(self, rhs: usize) -> Self::Output {
        Self::new(self.addr + rhs)
    }
}

/// Offsets the [`RawPage`] by `rhs` pages.
const impl Add<usize> for RawPage {
    type Output = Self;

    #[track_caller]
    fn add(self, rhs: usize) -> Self::Output {
        Self {
            inner: self.inner + PAGE_SIZE*rhs
        }
    }
}

/// Offsets the [`RawPage`] by `rhs` pages.
const impl Add<isize> for RawPage {
	type Output = Self;

	#[track_caller]
	fn add(self, rhs: isize) -> Self::Output {
		Self {
			inner: self.inner + (PAGE_SIZE as isize)*rhs
		}
	}
}

/// Offsets the [`RawFrame`] by `rhs` frames.
const impl Add<usize> for RawFrame {
    type Output = Self;

    #[track_caller]
    fn add(self, rhs: usize) -> Self::Output {
        Self {
            inner: self.inner + PAGE_SIZE*rhs
        }
    }
}

/// Offsets the [`RawFrame`] by `rhs` frames.
const impl Add<isize> for RawFrame {
	type Output = Self;

	#[track_caller]
	fn add(self, rhs: isize) -> Self::Output {
        Self {
			inner: self.inner + (PAGE_SIZE as isize)*rhs
		}
	}
}

/// Offsets the [`RawPage`] by `rhs` pages.
const impl Sub<usize> for RawPage {
    type Output = Self;

    #[track_caller]
    fn sub(self, rhs: usize) -> Self::Output {
        Self {
            inner: self.inner - PAGE_SIZE*rhs
        }
    }
}

/// Offsets the [`RawFrame`] by `rhs` frames.
const impl Sub<usize> for RawFrame {
    type Output = Self;

    #[track_caller]
    fn sub(self, rhs: usize) -> Self::Output {
        Self {
            inner: self.inner - PAGE_SIZE*rhs
        }
    }
}

const impl Add<isize> for VirtualAddress {
    type Output = Self;

    #[track_caller]
    fn add(self, rhs: isize) -> Self::Output {
        #[cfg(debug_assertions)]
        return Self::new(
            self.addr.checked_add_signed(rhs)
                    .expect("attempt to add with overflow")
        );

        #[cfg(not(debug_assertions))]
        return Self::new(
            self.addr.wrapping_add_signed(rhs)
        );
    }
}

const impl Add<isize> for PhysicalAddress {
    type Output = Self;

    #[track_caller]
    fn add(self, rhs: isize) -> Self::Output {
        #[cfg(debug_assertions)]
        return Self::new(
            self.addr.checked_add_signed(rhs)
                      .expect("attempt to add with overflow")
        );

        #[cfg(not(debug_assertions))]
        return Self::new(
            self.addr.wrapping_add_signed(rhs)
        );
    }
}

const impl Sub<usize> for VirtualAddress {
    type Output = Self;

    fn sub(self, rhs: usize) -> Self::Output {
        Self::new(self.addr - rhs)
    }
}

const impl Sub<usize> for PhysicalAddress {
    type Output = Self;

    fn sub(self, rhs: usize) -> Self::Output {
        Self::new(self.addr - rhs)
    }
}

const impl Sub<isize> for VirtualAddress {
    type Output = Self;

    #[track_caller]
    fn sub(self, rhs: isize) -> Self::Output {
        #[cfg(debug_assertions)]
        return Self::new(
            self.addr.checked_add_signed(-rhs)
                .expect("attempt to add with overflow")
        );

        #[cfg(not(debug_assertions))]
        return Self::new(
            self.addr.wrapping_add_signed(-rhs)
        );
    }
}

const impl Sub<isize> for PhysicalAddress {
    type Output = Self;

    #[track_caller]
    fn sub(self, rhs: isize) -> Self::Output {
        #[cfg(debug_assertions)]
        return Self::new(
            self.addr.checked_add_signed(-rhs)
                .expect("attempt to add with overflow")
        );

        #[cfg(not(debug_assertions))]
        return Self::new(
            self.addr.wrapping_add_signed(-rhs)
        );
    }
}

/// Returns the number of bytes between `self` and `rhs`.
const impl Sub<Self> for VirtualAddress {
    type Output = usize;

    fn sub(self, rhs: Self) -> Self::Output {
        self.addr - rhs.addr
    }
}

/// Returns the number of bytes between `self` and `rhs`.
const impl Sub<Self> for PhysicalAddress {
    type Output = usize;

    fn sub(self, rhs: Self) -> Self::Output {
        self.addr - rhs.addr
    }
}

/// Returns the number of frames between `self` and `rhs`.
const impl Sub<Self> for RawFrame {
    type Output = usize;

    fn sub(self, rhs: Self) -> Self::Output {
        (self.addr - rhs.addr) / PAGE_SIZE
    }
}

/// Returns the number of pages between `self` and `rhs`.
const impl Sub<Self> for RawPage {
    type Output = usize;

    fn sub(self, rhs: Self) -> Self::Output {
        (self.addr - rhs.addr) / PAGE_SIZE
    }
}

/*
#[stable(feature = "kernel_core_api", since = "1.0.0")]
impl Add<usize> for Page {
    type Output = Page;

    fn add(self, rhs: usize) -> Self::Output {
        Page {
            base: unsafe { (self.base + rhs * PAGE_SIZE).align_unchecked() }
        }
    }
}

#[stable(feature = "kernel_core_api", since = "1.0.0")]
impl Add<usize> for Frame {
    type Output = Frame;

    fn add(self, rhs: usize) -> Self::Output {
        Frame {
            base: unsafe { (self.base + rhs * PAGE_SIZE).align_unchecked() }
        }
    }
}

#[stable(feature = "kernel_core_api", since = "1.0.0")]
impl Add<isize> for Page {
    type Output = Page;

    fn add(self, rhs: isize) -> Self::Output {
        Page {
            base: unsafe { (self.base + rhs * (PAGE_SIZE as isize)).align_unchecked() }
        }
    }
}

#[stable(feature = "kernel_core_api", since = "1.0.0")]
impl Add<isize> for Frame {
    type Output = Frame;

    fn add(self, rhs: isize) -> Self::Output {
        Frame {
            base: unsafe { (self.base + rhs * (PAGE_SIZE as isize)).align_unchecked() }
        }
    }
}

#[stable(feature = "kernel_core_api", since = "1.0.0")]
impl Sub<usize> for Page {
    type Output = Page;

    fn sub(self, rhs: usize) -> Self::Output {
        Page {
            base: unsafe { (self.base - rhs * PAGE_SIZE).align_unchecked() }
        }
    }
}

#[stable(feature = "kernel_core_api", since = "1.0.0")]
impl Sub<usize> for Frame {
    type Output = Frame;

    fn sub(self, rhs: usize) -> Self::Output {
        Frame {
            base: unsafe { (self.base - rhs * PAGE_SIZE).align_unchecked() }
        }
    }
}

#[stable(feature = "kernel_core_api", since = "1.0.0")]
impl Sub<isize> for Page {
    type Output = Page;

    fn sub(self, rhs: isize) -> Self::Output {
        Page {
            base: unsafe { (self.base - rhs * (PAGE_SIZE as isize)).align_unchecked() }
        }
    }
}

#[stable(feature = "kernel_core_api", since = "1.0.0")]
impl Sub<isize> for Frame {
    type Output = Frame;

    fn sub(self, rhs: isize) -> Self::Output {
        Frame {
            base: unsafe { (self.base - rhs * (PAGE_SIZE as isize)).align_unchecked() }
        }
    }
}

#[stable(feature = "kernel_core_api", since = "1.0.0")]
impl Sub<Page> for Page {
    type Output = usize;

    fn sub(self, rhs: Page) -> Self::Output {
        (self.base.addr - rhs.base.addr) / PAGE_SIZE
    }
}

#[stable(feature = "kernel_core_api", since = "1.0.0")]
impl Sub<Frame> for Frame {
    type Output = usize;

    fn sub(self, rhs: Frame) -> Self::Output {
        (self.base.addr - rhs.base.addr) / PAGE_SIZE
    }
}

#[stable(feature = "kernel_core_api", since = "1.0.0")]
impl Step for Frame {
    fn steps_between(start: &Self, end: &Self) -> (usize, Option<usize>) {
        let steps = end.base.addr.checked_sub(start.base.addr)
            .map(|diff| diff / PAGE_SIZE);
        match steps {
            Some(s) => (s, Some(s)),
            None => (0, None),
            // Never need to return `(usize::MAX, None)` as difference between two `usize`s inside `PhysicalAddress` can't be bigger than `usize::MAX`
        }
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

#[stable(feature = "kernel_core_api", since = "1.0.0")]
impl Step for Page {
    fn steps_between(start: &Self, end: &Self) -> (usize, Option<usize>) {
        let steps = end.base.addr.checked_sub(start.base.addr)
            .map(|diff| diff / PAGE_SIZE);
        match steps {
            Some(s) => (s, Some(s)),
            None => (0, None),
        }
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
*/
