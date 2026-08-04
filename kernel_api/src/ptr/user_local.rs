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
	ptr: T,
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
	#[must_use]
	pub const unsafe fn new(ptr: *const T) -> Self {
		Self {
			ptr,
		}
	}

	/// Returns `true` if the pointer has a null address.
	#[must_use]
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
	/// See safety requirements for [`<*const T>::offset()`](pointer::offset).
	#[must_use = "this returns the result of the operation, without modifying the original"]
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
	/// See safety requirements for [`<*const T>::byte_offset()`](pointer::byte_offset).
	#[must_use = "this returns the result of the operation, without modifying the original"]
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
	/// See safety requirements for [`<*const T>::add()`](pointer::add).
	#[must_use = "this returns the result of the operation, without modifying the original"]
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
	/// See safety requirements for [`<*const T>::byte_add()`](pointer::byte_add).
	#[must_use = "this returns the result of the operation, without modifying the original"]
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
	/// See safety requirements for [`<*const T>::sub()`](pointer::sub).
	#[must_use = "this returns the result of the operation, without modifying the original"]
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
	/// See safety requirements for [`<*const T>::byte_sub()`](pointer::byte_sub).
	#[must_use = "this returns the result of the operation, without modifying the original"]
	pub const unsafe fn byte_sub(self, count: usize) -> Self {
		Self {
			// SAFETY: this function has the same safety requirements as `<*const T>::byte_sub()`
			ptr: unsafe { self.ptr.byte_sub(count) },
		}
	}

	/// Reads the value pointed to by `self`.
	///
	/// # Errors
	///
	/// Returns a [`PointerError`] if memory access failed.
	pub fn read(self) -> Result<T, PointerError> where T: Sized + Copy {
		match size_of::<T>() {
			// SAFETY: `self.ptr` must be a valid value of `T` and we checked it has the same size as u8
			1 => unsafe {
				impls::checked_read_1(self.ptr.cast())
						.map(|val| (&raw const val).cast::<T>().read())
			},
			// SAFETY: `self.ptr` must be a valid value of `T` and we checked it has the same size as u16
			2 => unsafe {
				impls::checked_read_2(self.ptr.cast())
						.map(|val| (&raw const val).cast::<T>().read())
			},
			// SAFETY: `self.ptr` must be a valid value of `T` and we checked it has the same size as u32
			4 => unsafe {
				impls::checked_read_4(self.ptr.cast())
						.map(|val| (&raw const val).cast::<T>().read())
			},
			// SAFETY: `self.ptr` must be a valid value of `T` and we checked it has the same size as u64
			#[cfg(target_pointer_width = "64")] 8 => unsafe {
				impls::checked_read_8(self.ptr.cast())
						.map(|val| (&raw const val).cast::<T>().read())
			},
			size => {
				let mut buf = MaybeUninit::<T>::uninit();
				impls::checked_memcpy(self.ptr.cast(), buf.as_mut_ptr().cast(), size)
						// SAFETY: `checked_memcpy` initialised the buffer if it returned Ok
						.map(|()| unsafe { buf.assume_init() })
			}
		}.ok_or_else(PointerError::new)
	}

	/// Casts a pointer to another type.
	///
	/// # Safety
	///
	/// If `self` has a valid address, then it must point to a valid bit pattern for `U` in the current
	/// address space.
	/// This means that in almost all cases, it is unsound to cast to a `LocalUser<*const U>` where `U`
	/// has a niche.
	///
	/// For all types with no invalid bit-patterns (i.e. all numeric types) it is sound to cast to
	/// a `LocalUser<*const U>`.
	#[must_use = "this returns the result of the operation, without modifying the original"]
	pub const unsafe fn cast<U>(self) -> LocalUser<*const U> {
		LocalUser {
			ptr: self.ptr.cast(),
		}
	}

	/// Casts to a writable pointer.
	#[must_use = "this returns the result of the operation, without modifying the original"]
	pub const fn cast_mut(self) -> LocalUser<*mut T> {
		LocalUser {
			ptr: self.ptr.cast_mut(),
		}
	}

	/// Returns whether the pointer is aligned to `align`.
	///
	/// For non-`Sized` pointees this operation considers only the data pointer,
	/// ignoring the metadata.
	///
	/// # Panics
	///
	/// The function panics if `align` is not a power-of-two (this includes 0).
	#[must_use]
	pub fn is_aligned_to(self, align: usize) -> bool {
		self.ptr.is_aligned_to(align)
	}

	/// Gets the "address" portion of the pointer.
	///
	/// This discards any provenance or metadata associated with the pointer.
	#[must_use]
	pub fn addr(self) -> VirtualAddress {
		VirtualAddress::new(self.ptr.addr())
	}

	/// Computes the offset that needs to be applied to the pointer in order to make it aligned to
	/// `align`.
	///
	/// If it is not possible to align the pointer, the implementation returns
	/// `usize::MAX`.
	///
	/// The offset is expressed in number of `T` elements, and not bytes.
	///
	/// There are no guarantees whatsoever that offsetting the pointer will not overflow or go
	/// beyond the allocation that the pointer points into. It is up to the caller to ensure that
	/// the returned offset is correct in all terms other than alignment.
	///
	/// # Panics
	///
	/// The function panics if `align` is not a power-of-two.
	#[must_use = "this returns the result of the operation, without modifying the original"]
	pub fn align_offset(self, align: usize) -> usize where T: Sized {
		self.ptr.align_offset(align)
	}
}

impl<T: ?Sized> LocalUser<*mut T> {
	/// Creates a new `LocalUser<*mut T>` with the provided address tied to the current address space.
	#[must_use]
	pub const fn new(ptr: *mut T) -> Self {
		Self {
			ptr,
		}
	}

	/// Returns `true` if the pointer has a null address.
	#[must_use]
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
	/// See safety requirements for [`<*mut T>::offset()`](pointer::offset).
	#[must_use = "this returns the result of the operation, without modifying the original"]
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
	/// See safety requirements for [`<*mut T>::byte_offset()`](pointer::byte_offset).
	#[must_use = "this returns the result of the operation, without modifying the original"]
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
	/// See safety requirements for [`<*mut T>::add()`](pointer::add).
	#[must_use = "this returns the result of the operation, without modifying the original"]
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
	/// See safety requirements for [`<*mut T>::byte_add()`](pointer::byte_add).
	#[must_use = "this returns the result of the operation, without modifying the original"]
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
	/// See safety requirements for [`<*mut T>::sub()`](pointer::sub).
	#[must_use = "this returns the result of the operation, without modifying the original"]
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
	/// See safety requirements for [`<*mut T>::byte_sub()`](pointer::byte_sub).
	#[must_use = "this returns the result of the operation, without modifying the original"]
	pub const unsafe fn byte_sub(self, count: usize) -> Self {
		Self {
			// SAFETY: this function has the same safety requirements as `<*mut T>::byte_sub()`
			ptr: unsafe { self.ptr.byte_sub(count) },
		}
	}

	/// Overwrites the memory pointed to by `self` with `value`.
	///
	/// # Errors
	///
	/// Returns a [`PointerError`] if memory access failed.
	pub fn write(self, value: T) -> Result<(), PointerError> where T: Sized {
		match size_of::<T>() {
			// SAFETY: `T` is of size 1 so valid to interpret as uu8
			1 => unsafe {
				impls::checked_write_1(self.ptr.cast(), (&raw const value).cast::<u8>().read())
			},
			// SAFETY: `T` is of size 2 so valid to interpret as u16
			2 => unsafe {
				impls::checked_write_2(self.ptr.cast(), (&raw const value).cast::<u16>().read())
			},
			// SAFETY: `T` is of size 4 so valid to interpret as u32
			4 => unsafe {
				impls::checked_write_4(self.ptr.cast(), (&raw const value).cast::<u32>().read())
			},
			// SAFETY: `T` is of size 8 so valid to interpret as u64
			#[cfg(target_pointer_width = "64")] 8 => unsafe {
				impls::checked_write_8(self.ptr.cast(), (&raw const value).cast::<u64>().read())
			},
			size => {
				impls::checked_memcpy((&raw const value).cast(), self.ptr.cast(), size)
			}
		}.ok_or_else(PointerError::new)?;
		core::mem::forget(value);
		Ok(())
	}

	/// Casts a pointer to another type.
	#[must_use = "this returns the result of the operation, without modifying the original"]
	pub const fn cast<U>(self) -> LocalUser<*mut U> {
		LocalUser {
			ptr: self.ptr.cast(),
		}
	}

	/// Casts to a readable pointer.
	///
	/// # Safety
	///
	/// `addr`, if valid, must point to a valid bit pattern for `T` in the current address space.
	/// This means that in almost all cases, it is unsound to cast to a `LocalUser<*const T>` where `T`
	/// has a niche.
	///
	/// For all types with no invalid bit-patterns (i.e. all numeric types) it is sound to cast to
	/// a `LocalUser<*const T>`.
	#[must_use = "this returns the result of the operation, without modifying the original"]
	pub const unsafe fn cast_const(self) -> LocalUser<*const T> {
		LocalUser {
			ptr: self.ptr.cast_const(),
		}
	}

	/// Returns whether the pointer is aligned to `align`.
	///
	/// For non-`Sized` pointees this operation considers only the data pointer,
	/// ignoring the metadata.
	///
	/// # Panics
	///
	/// The function panics if `align` is not a power-of-two (this includes 0).
	#[must_use]
	pub fn is_aligned_to(self, align: usize) -> bool {
		self.ptr.is_aligned_to(align)
	}

	/// Gets the "address" portion of the pointer.
	///
	/// This discards any provenance or metadata associated with the pointer.
	#[must_use]
	pub fn addr(self) -> VirtualAddress {
		VirtualAddress::new(self.ptr.addr())
	}

	/// Computes the offset that needs to be applied to the pointer in order to make it aligned to
	/// `align`.
	///
	/// If it is not possible to align the pointer, the implementation returns
	/// `usize::MAX`.
	///
	/// The offset is expressed in number of `T` elements, and not bytes.
	///
	/// There are no guarantees whatsoever that offsetting the pointer will not overflow or go
	/// beyond the allocation that the pointer points into. It is up to the caller to ensure that
	/// the returned offset is correct in all terms other than alignment.
	///
	/// # Panics
	///
	/// The function panics if `align` is not a power-of-two.
	#[must_use = "this returns the result of the operation, without modifying the original"]
	pub fn align_offset(self, align: usize) -> usize where T: Sized {
		self.ptr.align_offset(align)
	}
}

impl<T> LocalUser<*const [T]> {
	/// Returns the length of the slice.
	///
	/// The returned value is the number of **elements**, not the number of bytes.
	#[must_use]
	pub const fn len(self) -> usize { self.ptr.len() }

	/// Returns `true` if the slice is empty.
	#[must_use]
	pub const fn is_empty(self) -> bool { self.ptr.is_empty() }

	/// Returns a pointer to the first element of the slice.
	#[must_use]
	pub const fn as_ptr(self) -> LocalUser<*const T> {
		LocalUser {
			ptr: self.ptr.as_ptr(),
		}
	}

	/// Allocates a buffer and reads the slice pointed to by `self` into it.
	///
	/// # Errors
	///
	/// Returns a [`PointerError`] if memory access failed.
	pub fn read_to_box(self) -> Result<Box<[T]>, PointerError> {
		let mut buf = Box::new_uninit_slice(self.len());
		let size = self.read_to_buffer(&mut buf)?;
		debug_assert_eq!(size, buf.len(), "box should be big enough");
		// SAFETY: `read_to_buffer` fills the buffer with `self.len()` values, and invariant of
		// `LocalUser<*const [T]>` is that the pointed to values are valid for `T`
		Ok(unsafe { buf.assume_init() })
	}

	/// Reads the slice pointed to by `self` into a buffer.
	///
	/// Returns the number of **elements** read if successful.
	///
	/// # Errors
	///
	/// Returns a [`PointerError`] if memory access failed.
	pub fn read_to_buffer(self, buffer: &mut [MaybeUninit<T>]) -> Result<usize, PointerError> {
		let count = min(buffer.len(), self.len());

		impls::checked_memcpy(
			self.ptr.cast(),
			buffer.as_mut_ptr().cast(),
			size_of::<T>() * count
		).ok_or_else(PointerError::new)?;

		Ok(count)
	}
}

impl<T> LocalUser<*mut [T]> {
	/// Returns the length of the slice.
	///
	/// The returned value is the number of **elements**, not the number of bytes.
	#[must_use]
	pub const fn len(self) -> usize { self.ptr.len() }

	/// Returns `true` if the slice is empty.
	#[must_use]
	pub const fn is_empty(self) -> bool { self.ptr.is_empty() }

	/// Returns a pointer to the first element of the slice.
	#[must_use]
	pub const fn as_mut_ptr(self) -> LocalUser<*mut T> {
		LocalUser {
			ptr: self.ptr.as_mut_ptr(),
		}
	}

	/// Overwrites the slice pointed to by `self` with the passed slice.
	///
	/// Returns the number of **elements** written if successful.
	///
	/// # Errors
	///
	/// Returns a [`PointerError`] if memory access failed.
	pub fn write_from_slice(self, slice: &[T]) -> Result<usize, PointerError> {
		let count = min(slice.len(), self.len());

		impls::checked_memcpy(
			slice.as_ptr().cast(),
			self.ptr.cast(),
			size_of::<T>() * count
		).ok_or_else(PointerError::new)?;

		Ok(count)
	}
}

/// Forms a [local user pointer](`crate::ptr#local-pointers`) to a slice from a pointer and a length.
///
/// The `len` argument is the number of **elements**, not the number of bytes.
///
/// # Safety
///
/// If `data` points to accessible memory, it must point to `len` elements of type `T`.
/// See [`LocalUser::<*const T>::new`](`LocalUser::<*const T>::new#safety`) for more details.
#[must_use]
pub const unsafe fn local_slice_from_raw_parts<T>(data: LocalUser<*const T>, len: usize) -> LocalUser<*const [T]> {
	let ptr = core::ptr::slice_from_raw_parts(data.ptr, len);
	LocalUser {
		ptr,
	}
}

/// Forms a mutable [local user pointer](`crate::ptr#local-pointers`) to a slice from a pointer and a length.
///
/// The `len` argument is the number of **elements**, not the number of bytes.
#[must_use]
pub const fn local_slice_from_raw_parts_mut<T>(data: LocalUser<*mut T>, len: usize) -> LocalUser<*mut [T]> {
	let ptr = core::ptr::slice_from_raw_parts_mut(data.ptr, len);
	LocalUser {
		ptr,
	}
}
