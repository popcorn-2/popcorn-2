use alloc::boxed::Box;
use core::cmp::min;
use core::fmt;
use core::fmt::Formatter;
use core::mem::MaybeUninit;
use crate::memory::VirtualAddress;
use crate::ptr::{impls, PointerError};

/// A pointer to a potentially invalid address, tied to the current address space.
///
/// While the pointer may safely point to an invalid or unaligned address, in the case that
/// it points to a valid address and is read from, then it is unsound for the address to not
/// hold a valid bit-pattern for the type `T`.
///
/// For example, it is always sound to create a `LocalUser<*const u8>` regardless of the address it points
/// to, and always safe to call `read()` on it, but it would be unsound to create a
/// `LocalUser<*const bool>`if it's not guaranteed that the pointed value is either `1` or `0`.
///
/// Additionally, to make the API require less `unsafe` for common cases, `LocalUser<*mut T>` is always
/// safe to construct, with the additional requirement that it becomes write-only.
#[derive(Clone, Copy)]
pub struct LocalUser<T> {
	pub(super) ptr: T,
}

// LocalUser pointer is only valid in the thread that created it
impl<T> !Send for LocalUser<T> {}
impl<T> !Sync for LocalUser<T> {}

impl<T: fmt::Pointer> fmt::Pointer for LocalUser<T> {
	fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
		fmt::Pointer::fmt(&self.ptr, f)
	}
}

impl<T: fmt::Pointer> fmt::Debug for LocalUser<T> {
	fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
		fmt::Pointer::fmt(&self.ptr, f)
	}
}

impl<T: ?Sized> LocalUser<*const T> {
	/// Creates a new `LocalUser<*const T>` with tied to the current address space.
	///
	/// # Safety
	///
	/// `addr`, if valid, must point to a valid bit pattern for `T` in the current address space.
	/// This means that in almost all cases, it is unsound to create a `LocalUser<*const T>` where `T`
	/// has a niche.
	///
	/// For all types with no invalid bit-patterns (i.e. all numeric types) it is sound to create
	/// a `LocalUser<*const T>`.
	pub unsafe fn new(addr: usize) -> Self where T: Sized {
		Self {
			ptr: addr as *const T,
		}
	}

	/// Returns `true` if the pointer has a null address
	pub const fn is_null(&self) -> bool {
		self.ptr.is_null()
	}

	/// Adds a signed offset to a pointer.
	///
	/// `count` is in units of T; e.g., a `count` of 3 represents a pointer
	/// offset of `3 * size_of::<T>()` bytes.
	///
	/// # Safety
	///
	/// See safety requirements for [`<*const T>::offset()`]
	pub const unsafe fn offset(self, count: isize) -> Self where T: Sized {
		Self {
			// SAFETY: this function has the same safety requirements as `<*const T>::offset()`
			ptr: unsafe { self.ptr.offset(count) },
		}
	}

	/// Adds a signed offset in bytes to a pointer.
	///
	/// # Safety
	///
	/// See safety requirements for [`<*const T>::byte_offset()`]
	pub const unsafe fn byte_offset(self, count: isize) -> Self {
		Self {
			// SAFETY: this function has the same safety requirements as `<*const T>::byte_offset()`
			ptr: unsafe { self.ptr.byte_offset(count) },
		}
	}

	/// Adds an offset to a pointer.
	///
	/// `count` is in units of T; e.g., a `count` of 3 represents a pointer
	/// offset of `3 * size_of::<T>()` bytes.
	///
	/// # Safety
	///
	/// See safety requirements for [`<*const T>::add()`]
	pub const unsafe fn add(self, count: usize) -> Self where T: Sized {
		Self {
			// SAFETY: this function has the same safety requirements as `<*const T>::add()`
			ptr: unsafe { self.ptr.add(count) },
		}
	}

	/// Adds an offset in bytes to a pointer.
	///
	/// # Safety
	///
	/// See safety requirements for [`<*const T>::byte_add()`]
	pub const unsafe fn byte_add(self, count: usize) -> Self {
		Self {
			// SAFETY: this function has the same safety requirements as `<*const T>::byte_add()`
			ptr: unsafe { self.ptr.byte_add(count) },
		}
	}

	/// Subtracts an offset from a pointer.
	///
	/// `count` is in units of T; e.g., a `count` of 3 represents a pointer
	/// offset of `3 * size_of::<T>()` bytes.
	///
	/// # Safety
	///
	/// See safety requirements for [`<*const T>::sub()`]
	pub const unsafe fn sub(self, count: usize) -> Self where T: Sized {
		Self {
			// SAFETY: this function has the same safety requirements as `<*const T>::sub()`
			ptr: unsafe { self.ptr.sub(count) },
		}
	}

	/// Subtracts an offset in bytes from a pointer.
	///
	/// # Safety
	///
	/// See safety requirements for [`<*const T>::byte_sub()`]
	pub const unsafe fn byte_sub(self, count: usize) -> Self {
		Self {
			// SAFETY: this function has the same safety requirements as `<*const T>::byte_sub()`
			ptr: unsafe { self.ptr.byte_sub(count) },
		}
	}

	/// Tries to read the value pointed to by the pointer
	///
	/// # Errors
	///
	/// If the pointer is invalid due to unmapped memory or similar, returns [`PointerError`]
	// todo: do we need the `Copy` bound
	pub fn read(self) -> Result<T, PointerError> where T: Sized + Copy {
		match size_of::<T>() {
			1 => unsafe {
				impls::checked_read_1(self.ptr.cast())
						.map(|val| (&val as *const MaybeUninit<u8>).cast::<T>().read())
			},
			2 => unsafe {
				impls::checked_read_2(self.ptr.cast())
						.map(|val| (&val as *const MaybeUninit<u16>).cast::<T>().read())
			},
			4 => unsafe {
				impls::checked_read_4(self.ptr.cast())
						.map(|val| (&val as *const MaybeUninit<u32>).cast::<T>().read())
			},
			#[cfg(target_pointer_width = "64")] 8 => unsafe {
				impls::checked_read_8(self.ptr.cast())
						.map(|val| (&val as *const MaybeUninit<u64>).cast::<T>().read())
			},
			size => {
				let mut buf = MaybeUninit::<T>::uninit();
				impls::checked_memcpy(self.ptr.cast(), buf.as_mut_ptr().cast(), size)
						.map(|_| unsafe { buf.assume_init() })
			}
		}.ok_or(PointerError {})
	}

	/// Casts a pointer to another type
	///
	/// # Safety
	///
	/// If `self` has a valid address, then it must point to a valid bit pattern for `U` in the current
	/// address space.
	/// This means that in almost all cases, it is unsound to cast to a `LocalUser<*const U>` where `U`
	/// has a niche.
	///
	/// For all types with no invalid bit-patterns (i.e. all numeric types) it is sound to cast to
	/// a `User<*const U>`.
	pub const unsafe fn cast<U>(self) -> LocalUser<*const U> {
		LocalUser {
			ptr: self.ptr.cast(),
		}
	}

	/// Casts to a writable pointer
	pub const fn cast_mut(self) -> LocalUser<*mut T> {
		LocalUser {
			ptr: self.ptr.cast_mut(),
		}
	}

	pub fn is_aligned_to(self, align: usize) -> bool {
		self.ptr.is_aligned_to(align)
	}

	pub fn addr(self) -> VirtualAddress {
		VirtualAddress::new(self.ptr.addr())
	}

	pub fn align_offset(self, align: usize) -> usize where T: Sized {
		self.ptr.align_offset(align)
	}
}

impl<T: ?Sized> LocalUser<*mut T> {
	/// Creates a new `LocalUser<*mut T>` with the provided address tied to the current address space
	pub fn new(addr: usize) -> Self where T: Sized {
		Self {
			ptr: addr as *mut T,
		}
	}

	/// Returns `true` if the pointer has a null address
	pub const fn is_null(&self) -> bool {
		self.ptr.is_null()
	}

	/// Adds a signed offset to a pointer.
	///
	/// `count` is in units of T; e.g., a `count` of 3 represents a pointer
	/// offset of `3 * size_of::<T>()` bytes.
	///
	/// # Safety
	///
	/// See safety requirements for [`<*mut T>::offset()`]
	pub const unsafe fn offset(self, count: isize) -> Self where T: Sized {
		Self {
			// SAFETY: this function has the same safety requirements as `<*mut T>::offset()`
			ptr: unsafe { self.ptr.offset(count) },
		}
	}

	/// Adds a signed offset in bytes to a pointer.
	///
	/// # Safety
	///
	/// See safety requirements for [`<*mut T>::byte_offset()`]
	pub const unsafe fn byte_offset(self, count: isize) -> Self {
		Self {
			// SAFETY: this function has the same safety requirements as `<*mut T>::byte_offset()`
			ptr: unsafe { self.ptr.byte_offset(count) },
		}
	}

	/// Adds an offset to a pointer.
	///
	/// `count` is in units of T; e.g., a `count` of 3 represents a pointer
	/// offset of `3 * size_of::<T>()` bytes.
	///
	/// # Safety
	///
	/// See safety requirements for [`<*mut T>::add()`]
	pub const unsafe fn add(self, count: usize) -> Self where T: Sized {
		Self {
			// SAFETY: this function has the same safety requirements as `<*mut T>::add()`
			ptr: unsafe { self.ptr.add(count) },
		}
	}

	/// Adds an offset in bytes to a pointer.
	///
	/// # Safety
	///
	/// See safety requirements for [`<*mut T>::byte_add()`]
	pub const unsafe fn byte_add(self, count: usize) -> Self {
		Self {
			// SAFETY: this function has the same safety requirements as `<*mut T>::byte_add()`
			ptr: unsafe { self.ptr.byte_add(count) },
		}
	}

	/// Subtracts an offset from a pointer.
	///
	/// `count` is in units of T; e.g., a `count` of 3 represents a pointer
	/// offset of `3 * size_of::<T>()` bytes.
	///
	/// # Safety
	///
	/// See safety requirements for [`<*mut T>::sub()`]
	pub const unsafe fn sub(self, count: usize) -> Self where T: Sized {
		Self {
			// SAFETY: this function has the same safety requirements as `<*mut T>::sub()`
			ptr: unsafe { self.ptr.sub(count) },
		}
	}

	/// Subtracts an offset in bytes from a pointer.
	///
	/// # Safety
	///
	/// See safety requirements for [`<*mut T>::byte_sub()`]
	pub const unsafe fn byte_sub(self, count: usize) -> Self {
		Self {
			// SAFETY: this function has the same safety requirements as `<*mut T>::byte_sub()`
			ptr: unsafe { self.ptr.byte_sub(count) },
		}
	}

	/// Tries to write to the value pointed to by the pointer
	///
	/// # Errors
	///
	/// If the pointer is invalid due to unmapped memory or similar, returns [`PointerError`]
	pub fn write(self, value: T) -> Result<(), PointerError> where T: Sized {
		match size_of::<T>() {
			1 => unsafe {
				impls::checked_write_1(self.ptr.cast(), (&value as *const T).cast::<MaybeUninit<u8>>().read())
			},
			2 => unsafe {
				impls::checked_write_2(self.ptr.cast(), (&value as *const T).cast::<MaybeUninit<u16>>().read())
			},
			4 => unsafe {
				impls::checked_write_4(self.ptr.cast(), (&value as *const T).cast::<MaybeUninit<u32>>().read())
			},
			#[cfg(target_pointer_width = "64")] 8 => unsafe {
				impls::checked_write_8(self.ptr.cast(), (&value as *const T).cast::<MaybeUninit<u64>>().read())
			},
			size => {
				impls::checked_memcpy((&value as *const T).cast(), self.ptr.cast(), size)
			}
		}.ok_or(PointerError {})
	}

	/// Casts a pointer to another type
	pub const fn cast<U>(self) -> LocalUser<*mut U> {
		LocalUser {
			ptr: self.ptr.cast(),
		}
	}

	/// Casts to a readable pointer
	///
	/// # Safety
	///
	/// `addr`, if valid, must point to a valid bit pattern for `T` in the current address space.
	/// This means that in almost all cases, it is unsound to cast to a `LocalUser<*const T>` where `T`
	/// has a niche.
	///
	/// For all types with no invalid bit-patterns (i.e. all numeric types) it is sound to cast to
	/// a `LocalUser<*const T>`.
	pub const unsafe fn cast_const(self) -> LocalUser<*const T> {
		LocalUser {
			ptr: self.ptr.cast_const(),
		}
	}

	pub fn is_aligned_to(self, align: usize) -> bool {
		self.ptr.is_aligned_to(align)
	}

	pub fn addr(self) -> VirtualAddress {
		VirtualAddress::new(self.ptr.addr())
	}

	pub fn align_offset(self, align: usize) -> usize where T: Sized {
		self.ptr.align_offset(align)
	}
}

impl<T> LocalUser<*const [T]> {
	pub const fn len(self) -> usize { self.ptr.len() }

	pub const fn is_empty(self) -> bool { self.ptr.is_empty() }

	pub const fn as_ptr(self) -> LocalUser<*const T> {
		LocalUser {
			ptr: self.ptr.as_ptr(),
		}
	}

	pub fn read_to_box(self) -> Result<Box<[T]>, PointerError> {
		let mut buf = Box::new_uninit_slice(self.len());
		let size = self.read_to_buffer(&mut buf)?;
		assert_eq!(size, buf.len(), "box should be big enough");
		Ok(unsafe { buf.assume_init() })
	}

	pub fn read_to_buffer(self, buffer: &mut [MaybeUninit<T>]) -> Result<usize, PointerError> {
		let count = min(buffer.len(), self.len());

		impls::checked_memcpy(
			self.ptr.cast(),
			buffer.as_mut_ptr().cast(),
			size_of::<T>() * count
		).ok_or(PointerError {})?;

		Ok(count)
	}
}

impl<T> LocalUser<*mut [T]> {
	pub const fn len(self) -> usize { self.ptr.len() }

	pub const fn is_empty(self) -> bool { self.ptr.is_empty() }

	pub const fn as_mut_ptr(self) -> LocalUser<*mut T> {
		LocalUser {
			ptr: self.ptr.as_mut_ptr(),
		}
	}

	pub fn write_from_slice(self, slice: &[T]) -> Result<usize, PointerError> {
		let count = min(slice.len(), self.len());

		impls::checked_memcpy(
			slice.as_ptr().cast(),
			self.ptr.cast(),
			size_of::<T>() * count
		).ok_or(PointerError {})?;

		Ok(count)
	}
}

/// # Safety
///
/// If `data` points to accessible memory, it must point to `len` elements of type `T`.
/// See [`LocalUser::<*const T>::new`](`LocalUser::<*const T>::new#safety`) for more details.
pub unsafe fn local_slice_from_raw_parts<T>(data: LocalUser<*const T>, len: usize) -> LocalUser<*const [T]> {
	let ptr = core::ptr::slice_from_raw_parts(data.ptr, len);
	LocalUser {
		ptr,
	}
}

pub fn local_slice_from_raw_parts_mut<T>(data: LocalUser<*mut T>, len: usize) -> LocalUser<*mut [T]> {
	let ptr = core::ptr::slice_from_raw_parts_mut(data.ptr, len);
	LocalUser {
		ptr,
	}
}
