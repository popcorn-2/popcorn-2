use core::mem::MaybeUninit;
use crate::ptr::impls;
use alloc::boxed::Box;
use core::ptr::NonNull;
use crate::bridge::paging::AddressSpaceInner;
use crate::memory::r#virtual::AddressSpace;

pub enum PointerError {
	InvalidAddress,
}

//#[derive(Copy, Clone, Debug, Ord, PartialOrd, Eq, PartialEq, Hash)]
pub struct User<T>(T, NonNull<AddressSpaceInner>);

fn __current_address_space() -> NonNull<AddressSpaceInner> { todo!("get current address space from percpu") }

macro_rules! user_ptr_impl_unsized {
	($ty: ident) => {
		pub fn new_in(from: * $ty T, address_space: &AddressSpace) -> Self {
			Self(from, address_space.as_ptr())
		}
		
		pub fn is_null(self) -> bool {
		    self.0.is_null()
	    }

	    pub fn cast<U>(self) -> User<* $ty U> {
		    User(self.0.cast(), self.1)
	    }

	    /*pub unsafe fn byte_offset(self, count: isize) -> Self {
		    User(self.0.byte_offset(count))
	    }

	    pub unsafe fn wrapping_byte_offset(self, count: isize) -> Self {
		    User(self.0.wrapping_byte_offset(count))
	    }
		
	    pub unsafe fn byte_offset_from(self, origin: Self) -> isize {
		    self.0.byte_offset_from(origin.0)
	    }*/
	};
}

macro_rules! user_ptr_impl_sized {
    ($ty: ident) => {
	    /*pub unsafe fn offset(self, count: isize) -> Self {
		    User(self.0.offset(count))
	    }

	    pub unsafe fn wrapping_offset(self, count: isize) -> Self {
		    User(self.0.wrapping_offset(count))
	    }

	    pub unsafe fn offset_from(self, origin: Self) -> isize {
		    self.0.offset_from(origin.0)
	    }*/
		
	    /// # Safety
	    /// 
	    /// The memory pointed to must be valid for a read of `T`
	    pub unsafe fn read(self) -> Result<T, PointerError> {
		    unsafe {
			    assert!(
				    crate::bridge::memory::__popcorn_check_address_space(self.1),
				    "Address space of User<*> should match current address space",
			    );
		    }
		    match size_of::<T>() {
			    1 => unsafe {
				    impls::checked_read_1(self.0.cast())
				        .map(|val| (&val as *const MaybeUninit<u8>).cast::<T>().read())
			    },
			    2 => unsafe {
				    impls::checked_read_2(self.0.cast())
				        .map(|val| (&val as *const MaybeUninit<u16>).cast::<T>().read())
			    },
			    4 => unsafe {
				    impls::checked_read_4(self.0.cast())
				        .map(|val| (&val as *const MaybeUninit<u32>).cast::<T>().read())
			    },
			    #[cfg(target_arch = "x86_64")] 8 => unsafe {
				    impls::checked_read_8(self.0.cast())
				        .map(|val| (&val as *const MaybeUninit<u64>).cast::<T>().read())
			    },
			    size => {
				    let mut buf = MaybeUninit::<T>::uninit();
				    impls::checked_memcpy(self.0.cast(), buf.as_mut_ptr().cast(), size)
				        .map(|_| unsafe { buf.assume_init() })
			    }
		    }.ok_or(PointerError::InvalidAddress)
	    }

	    /// # Safety
	    ///
	    /// The memory pointed to must be valid for a read of `T`
	    pub unsafe fn read_unaligned(self) -> Result<T, PointerError> {
		    unsafe {
			    assert!(
				    crate::bridge::memory::__popcorn_check_address_space(self.1),
				    "Address space of User<*> should match current address space",
			    );
		    }
		    todo!()
	    }

	    /// # Safety
	    ///
	    /// The memory pointed to must be valid for a read of `T`
	    /// 
	    /// `dest` must be valid to write to as defined by [`core::ptr::write`]
	    pub unsafe fn copy_to_nonoverlapping(self, dest: *mut T, count: usize) -> Result<(), PointerError> {
		    unsafe {
			    assert!(
				    crate::bridge::memory::__popcorn_check_address_space(self.1),
				    "Address space of User<*> should match current address space",
			    );
		    }
		    impls::checked_memcpy(self.0.cast(), dest.cast(), size_of::<T>() * count).ok_or(PointerError::InvalidAddress)
	    }

	    pub fn copy_to_user(self, dest: User<*mut T>, _count: usize) -> Result<(), PointerError> {
		    unsafe {
			    assert!(
				    crate::bridge::memory::__popcorn_check_address_space(self.1),
				    "Address space of User<*> should match current address space",
			    );
			    assert!(
				    crate::bridge::memory::__popcorn_check_address_space(dest.1),
				    "Address space of User<*> should match current address space",
			    );
		    }
		    todo!()
	    }
    };
}

macro_rules! user_ptr_impl_slice {
    ($ty: ident) => {
	    /// # Safety
	    ///
	    /// The memory pointed to must be valid for a read of `[T]`
	    pub unsafe fn read_to_buffer(self) -> Result<Box<[T]>, PointerError> {
		    unsafe {
			    assert!(
				    crate::bridge::memory::__popcorn_check_address_space(self.1),
				    "Address space of User<*> should match current address space",
			    );
		    }
		    let len = self.0.len();
		    let mut buf = Box::new_uninit_slice(len);
		    self.cast::<T>().copy_to_nonoverlapping(buf.as_mut_ptr().cast(), len)
		        .map(|_| buf.assume_init())
	    }
	    
	    pub fn is_empty(self) -> bool {
		    self.len() == 0
	    }
	    
	    pub fn len(self) -> usize {
		    self.0.len()
	    }
    };
}

impl<T: ?Sized> User<*const T> {
	user_ptr_impl_unsized!(const);

	pub fn cast_mut(self) -> User<*mut T> {
		User(self.0.cast_mut(), self.1)
	}
}

impl<T: ?Sized> User<*mut T> {
	user_ptr_impl_unsized!(mut);

	pub fn cast_const(self) -> User<*const T> {
		User(self.0.cast_const(), self.1)
	}
}

impl<T> User<*const T> {
	user_ptr_impl_sized!(const);
}

impl<T> User<*mut T> {
	user_ptr_impl_sized!(mut);

	pub fn write(self, val: T) -> Result<(), PointerError> {
		unsafe {
			assert!(
				crate::bridge::memory::__popcorn_check_address_space(self.1),
				"Address space of User<*> should match current address space",
			);
		}
		match size_of::<T>() {
			1 => unsafe {
				impls::checked_write_1(self.0.cast(), (&val as *const T).cast::<MaybeUninit<u8>>().read())
			},
			2 => unsafe {
				impls::checked_write_2(self.0.cast(), (&val as *const T).cast::<MaybeUninit<u16>>().read())
			},
			4 => unsafe {
				impls::checked_write_4(self.0.cast(), (&val as *const T).cast::<MaybeUninit<u32>>().read())
			},
			#[cfg(target_arch = "x86_64")] 8 => unsafe {
				impls::checked_write_8(self.0.cast(), (&val as *const T).cast::<MaybeUninit<u64>>().read())
			},
			size => {
				impls::checked_memcpy((&val as *const T).cast(), self.0.cast(), size)
			}
		}.ok_or(PointerError::InvalidAddress)
	}

	pub fn write_unaligned(self, _val: T) -> Result<(), PointerError> {
		unsafe {
			assert!(
				crate::bridge::memory::__popcorn_check_address_space(self.1),
				"Address space of User<*> should match current address space",
			);
		}
		todo!()
	}

	/// # Safety
	///
	/// `src` must be valid for a read of `T` as defined by [`core::ptr::read`]
	pub fn copy_from_nonoverlapping(self, src: *const T, count: usize) -> Result<(), PointerError> {
		unsafe {
			assert!(
				crate::bridge::memory::__popcorn_check_address_space(self.1),
				"Address space of User<*> should match current address space",
			);
		}
		impls::checked_memcpy( src.cast(), self.0.cast(),size_of::<T>() * count).ok_or(PointerError::InvalidAddress)
	}

	pub fn copy_from_user(self, src: User<*const T>, count: usize) -> Result<(), PointerError> {
		src.copy_to_user(self, count)
	}
}

impl<T> User<*const [T]> {
	user_ptr_impl_slice!(const);
}

impl<T> User<*mut [T]> {
	user_ptr_impl_slice!(mut);
}

pub fn slice_from_raw_parts<T>(data: User<*const T>, len: usize) -> User<*const [T]> {
	User(core::ptr::slice_from_raw_parts(data.0, len), data.1)
}

pub fn slice_from_raw_parts_mut<T>(data: User<*mut T>, len: usize) -> User<*mut [T]> {
	User(core::ptr::slice_from_raw_parts_mut(data.0, len), data.1)
}
