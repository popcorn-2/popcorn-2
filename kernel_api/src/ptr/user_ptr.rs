use crate::ptr::{impls, LocalUser, PointerError};
use core::fmt;
use core::fmt::Formatter;
use crate::address_space;
use crate::memory::VirtualAddress;

/// A pointer to a potentially invalid address, tied to a specific address space.
///
/// If the address space is known to be the current address space, [`LocalUser<T>`](`super::LocalUser`)
/// can be used instead, which reduces the overhead when reading and writing.
///
/// While the pointer may safely point to an invalid or unaligned address, in the case that
/// it points to a valid address and is read from, then it is unsound for the address to not
/// hold a valid bit-pattern for the type `T`.
///
/// For example, it is always sound to create a `User<*const u8>` regardless of the address it points
/// to, and always safe to call `read()` on it, but it would be unsound to create a
/// `User<*const bool>`if it's not guaranteed that the pointed value is either `1` or `0`.
///
/// Additionally, to make the API require less `unsafe` for common cases, `User<*mut T>` is always
/// safe to construct, with the additional requirement that it becomes write-only.
// todo: maybe don't allow access to kernel addresses
#[derive(Clone, Copy)]
pub struct User<'a, T> {
	ptr: T,
	address_space: Option<&'a address_space::User>,
}

// SAFETY: All access goes through checked functions which ensure the pointer is valid
// to dereference in the current address space. Therefore, when used on a different thread
// the pointer will either be fine to use (as long as the type it points to is safe to
// read from another thread) or it will return an error
unsafe impl<T: Send + ?Sized> Send for User<'_, *mut T> {}

// SAFETY: see above comment on `*mut T`
unsafe impl<T: Send + ?Sized> Send for User<'_, *const T> {}

impl<T: fmt::Pointer> fmt::Pointer for User<'_, T> {
	fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
		fmt::Pointer::fmt(&self.ptr, f)
	}
}

impl<T: fmt::Pointer> fmt::Debug for User<'_, T> {
	fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
		fmt::Pointer::fmt(&self.ptr, f)
	}
}

/// Produces a null pointer.
///
/// See [`null_mut()`] for more details.
impl<T: core::ptr::Thin + ?Sized> Default for User<'_, *mut T> {
	fn default() -> Self {
		null_mut()
	}
}

/// Produces a null pointer.
///
/// See [`null()`] for more details.
impl<T: core::ptr::Thin + ?Sized> Default for User<'_, *const T> {
	fn default() -> Self {
		null()
	}
}

/// Produces a null [`User<*mut T>`].
///
/// This will always return an error when writing to.
#[must_use]
pub const fn null_mut<T: core::ptr::Thin + ?Sized>() -> User<'static, *mut T> {
	User {
		ptr: core::ptr::null_mut(),
		address_space: None,
	}
}

/// Produces a null [`User<*const T>`].
///
/// This will always return an error when reading from.
#[must_use]
pub const fn null<T: core::ptr::Thin + ?Sized>() -> User<'static, *const T> {
	User {
		ptr: core::ptr::null(),
		address_space: None,
	}
}

/// Pointer equality is by address space, and [`<*mut T>::eq`].
impl<T: ?Sized> PartialEq for User<'_, *mut T> {
	#[expect(ambiguous_wide_pointer_comparisons, reason = "want same behaviour as `PartialEq` on raw pointer")]
	fn eq(&self, other: &Self) -> bool {
		let ptr_eq = self.ptr == other.ptr;
		let address_space_eq = self.address_space.is_some_and(
			|this| other.address_space.is_some_and(|other| address_space::User::ptr_eq(this, other))
		);
		ptr_eq && address_space_eq
	}
}

/// Pointer equality is by address space, and [`<*const T>::eq`].
impl<T: ?Sized> PartialEq for User<'_, *const T> {
	#[expect(ambiguous_wide_pointer_comparisons, reason = "want same behaviour as `PartialEq` on raw pointer")]
	fn eq(&self, other: &Self) -> bool {
		let ptr_eq = self.ptr == other.ptr;
		let address_space_eq = self.address_space.is_some_and(
			|this| other.address_space.is_some_and(|other| address_space::User::ptr_eq(this, other))
		);
		ptr_eq && address_space_eq
	}
}

/*
impl<T: ?Sized> PartialOrd for User<'_, *mut T> {
	#[expect(ambiguous_wide_pointer_comparisons, reason = "want same behaviour as `PartialOrd` on raw pointer")]
	fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
		if self.address_space != other.address_space { None }
		else { self.ptr.partial_cmp(&other.ptr) }
	}
}

impl<T: ?Sized> PartialOrd for User<'_, *const T> {
	#[expect(ambiguous_wide_pointer_comparisons, reason = "want same behaviour as `PartialOrd` on raw pointer")]
	fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
		if self.address_space != other.address_space { None }
		else { self.ptr.partial_cmp(&other.ptr) }
	}
}
*/

impl<T: ?Sized> Eq for User<'_, *const T> {}
impl<T: ?Sized> Eq for User<'_, *mut T> {}

impl<'a, T: ?Sized> User<'a, *const T> {
	/// Creates a new `User<*const T>` with the provided address tied to the provided address space.
	///
	/// # Safety
	///
	/// `addr`, if valid, must point to a valid bit pattern for `T` in the current address space.
	/// This means that in almost all cases, it is unsound to create a `User<*const T>` where `T`
	/// has a niche.
	///
	/// For all types with no invalid bit-patterns (i.e. all numeric types) it is sound to create
	/// a `User<*const T>`.
	pub(crate) const unsafe fn new(ptr: *const T, address_space: &'a address_space::User) -> Self where T: Sized {
		Self {
			ptr,
			address_space: Some(address_space),
		}
	}

	/// Returns `true` if the pointer has either a null address or address space.
	#[must_use]
	pub const fn is_null(&self) -> bool {
		self.ptr.is_null() || self.address_space.is_none()
	}

	/// Adds a signed offset to a pointer.
	///
	/// `count` is in units of T; e.g., a `count` of 3 represents a pointer
	/// offset of `3 * size_of::<T>()` bytes.
	///
	/// # Safety
	///
	/// See safety requirements for [`<*const T>::offset()`].
	#[must_use = "this returns the result of the operation, without modifying the original"]
	pub const unsafe fn offset(self, count: isize) -> Self where T: Sized {
		Self {
			// SAFETY: this function has the same safety requirements as `<*const T>::offset()`
			ptr: unsafe { self.ptr.offset(count) },
			address_space: self.address_space,
		}
	}

	/// Adds a signed offset in bytes to a pointer.
	///
	/// # Safety
	///
	/// See safety requirements for [`<*const T>::byte_offset()`].
	#[must_use = "this returns the result of the operation, without modifying the original"]
	pub const unsafe fn byte_offset(self, count: isize) -> Self {
		Self {
			// SAFETY: this function has the same safety requirements as `<*const T>::byte_offset()`
			ptr: unsafe { self.ptr.byte_offset(count) },
			address_space: self.address_space,
		}
	}

	/// Adds an offset to a pointer.
	///
	/// `count` is in units of T; e.g., a `count` of 3 represents a pointer
	/// offset of `3 * size_of::<T>()` bytes.
	///
	/// # Safety
	///
	/// See safety requirements for [`<*const T>::add()`].
	#[must_use = "this returns the result of the operation, without modifying the original"]
	pub const unsafe fn add(self, count: usize) -> Self where T: Sized {
		Self {
			// SAFETY: this function has the same safety requirements as `<*const T>::add()`
			ptr: unsafe { self.ptr.add(count) },
			address_space: self.address_space,
		}
	}

	/// Adds an offset in bytes to a pointer.
	///
	/// # Safety
	///
	/// See safety requirements for [`<*const T>::byte_add()`].
	#[must_use = "this returns the result of the operation, without modifying the original"]
	pub const unsafe fn byte_add(self, count: usize) -> Self {
		Self {
			// SAFETY: this function has the same safety requirements as `<*const T>::byte_add()`
			ptr: unsafe { self.ptr.byte_add(count) },
			address_space: self.address_space,
		}
	}

	/// Subtracts an offset from a pointer.
	///
	/// `count` is in units of T; e.g., a `count` of 3 represents a pointer
	/// offset of `3 * size_of::<T>()` bytes.
	///
	/// # Safety
	///
	/// See safety requirements for [`<*const T>::sub()`].
	#[must_use = "this returns the result of the operation, without modifying the original"]
	pub const unsafe fn sub(self, count: usize) -> Self where T: Sized {
		Self {
			// SAFETY: this function has the same safety requirements as `<*const T>::sub()`
			ptr: unsafe { self.ptr.sub(count) },
			address_space: self.address_space,
		}
	}

	/// Subtracts an offset in bytes from a pointer.
	///
	/// # Safety
	///
	/// See safety requirements for [`<*const T>::byte_sub()`].
	#[must_use = "this returns the result of the operation, without modifying the original"]
	pub const unsafe fn byte_sub(self, count: usize) -> Self {
		Self {
			// SAFETY: this function has the same safety requirements as `<*const T>::byte_sub()`
			ptr: unsafe { self.ptr.byte_sub(count) },
			address_space: self.address_space,
		}
	}

	/*
	/// Tries to read the value pointed to by the pointer
	///
	/// # Errors
	///
	/// If the pointer is invalid due to unmapped memory or similar, returns [`PointerError`]
	// todo: do we need the `Copy` bound
	pub fn read(self) -> Result<T, PointerError> where T: Sized + Copy {
		assert!(
			crate::bridge::address_space::is_current(self.address_space),
			"Address space of User<*> should match current address space",
		);
		
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
	}*/

	/*
	/// Reads the value pointed to by `self`.
	///
	/// # Errors
	///
	/// Returns a [`PointerError`] if memory access failed.
	pub fn read_other_address_space(self) -> Result<T, PointerError> where T: Sized + Copy {
		todo!()
	}*/

	/// Casts a pointer to another type.
	///
	/// # Safety
	///
	/// If `self` has a valid address, then it must point to a valid bit pattern for `U` in the current
	/// address space.
	/// This means that in almost all cases, it is unsound to cast to a `User<*const U>` where `U`
	/// has a niche.
	///
	/// For all types with no invalid bit-patterns (i.e. all numeric types) it is sound to cast to
	/// a `User<*const U>`.
	#[must_use = "this returns the result of the operation, without modifying the original"]
	pub const unsafe fn cast<U>(self) -> User<'a, *const U> {
		User {
			ptr: self.ptr.cast(),
			address_space: self.address_space,
		}
	}

	/// Casts to a writable pointer.
	#[must_use = "this returns the result of the operation, without modifying the original"]
	pub const fn cast_mut(self) -> User<'a, *mut T> {
		User {
			ptr: self.ptr.cast_mut(),
			address_space: self.address_space,
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

impl<'a, T: ?Sized> User<'a, *mut T> {
	/// Creates a new `User<*mut T>` with the provided address tied to the provided address space.
	pub(crate) const fn new(ptr: *mut T, address_space: &'a address_space::User) -> Self where T: Sized {
		Self {
			ptr,
			address_space: Some(address_space),
		}
	}

	/// Returns `true` if the pointer has either a null address or address space.
	#[must_use]
	pub const fn is_null(&self) -> bool {
		self.ptr.is_null() || self.address_space.is_none()
	}

	/// Adds a signed offset to a pointer.
	///
	/// `count` is in units of T; e.g., a `count` of 3 represents a pointer
	/// offset of `3 * size_of::<T>()` bytes.
	///
	/// # Safety
	///
	/// See safety requirements for [`<*mut T>::offset()`].
	#[must_use = "this returns the result of the operation, without modifying the original"]
	pub const unsafe fn offset(self, count: isize) -> Self where T: Sized {
		Self {
			// SAFETY: this function has the same safety requirements as `<*mut T>::offset()`
			ptr: unsafe { self.ptr.offset(count) },
			address_space: self.address_space,
		}
	}

	/// Adds a signed offset in bytes to a pointer.
	///
	/// # Safety
	///
	/// See safety requirements for [`<*mut T>::byte_offset()`].
	#[must_use = "this returns the result of the operation, without modifying the original"]
	pub const unsafe fn byte_offset(self, count: isize) -> Self {
		Self {
			// SAFETY: this function has the same safety requirements as `<*mut T>::byte_offset()`
			ptr: unsafe { self.ptr.byte_offset(count) },
			address_space: self.address_space,
		}
	}

	/// Adds an offset to a pointer.
	///
	/// `count` is in units of T; e.g., a `count` of 3 represents a pointer
	/// offset of `3 * size_of::<T>()` bytes.
	///
	/// # Safety
	///
	/// See safety requirements for [`<*mut T>::add()`].
	#[must_use = "this returns the result of the operation, without modifying the original"]
	pub const unsafe fn add(self, count: usize) -> Self where T: Sized {
		Self {
			// SAFETY: this function has the same safety requirements as `<*mut T>::add()`
			ptr: unsafe { self.ptr.add(count) },
			address_space: self.address_space,
		}
	}

	/// Adds an offset in bytes to a pointer.
	///
	/// # Safety
	///
	/// See safety requirements for [`<*mut T>::byte_add()`].
	#[must_use = "this returns the result of the operation, without modifying the original"]
	pub const unsafe fn byte_add(self, count: usize) -> Self {
		Self {
			// SAFETY: this function has the same safety requirements as `<*mut T>::byte_add()`
			ptr: unsafe { self.ptr.byte_add(count) },
			address_space: self.address_space,
		}
	}

	/// Subtracts an offset from a pointer.
	///
	/// `count` is in units of T; e.g., a `count` of 3 represents a pointer
	/// offset of `3 * size_of::<T>()` bytes.
	///
	/// # Safety
	///
	/// See safety requirements for [`<*mut T>::sub()`].
	#[must_use = "this returns the result of the operation, without modifying the original"]
	pub const unsafe fn sub(self, count: usize) -> Self where T: Sized {
		Self {
			// SAFETY: this function has the same safety requirements as `<*mut T>::sub()`
			ptr: unsafe { self.ptr.sub(count) },
			address_space: self.address_space,
		}
	}

	/// Subtracts an offset in bytes from a pointer.
	///
	/// # Safety
	///
	/// See safety requirements for [`<*mut T>::byte_sub()`].
	#[must_use = "this returns the result of the operation, without modifying the original"]
	pub const unsafe fn byte_sub(self, count: usize) -> Self {
		Self {
			// SAFETY: this function has the same safety requirements as `<*mut T>::byte_sub()`
			ptr: unsafe { self.ptr.byte_sub(count) },
			address_space: self.address_space,
		}
	}

	/*
	/// Tries to write to the value pointed to by the pointer
	///
	/// # Errors
	///
	/// If the pointer is invalid due to unmapped memory or similar, returns [`PointerError`]
	pub fn write(self, value: T) -> Result<(), PointerError> where T: Sized {
		trace!("direct write <val> -> {:#p}", self);
		
		assert!(
			crate::bridge::address_space::is_current(self.address_space),
			"Address space of User<*> should match current address space",
		);
			
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
	}*/

	/// Overwrites the memory pointed to by `self` with `value`.
	///
	/// # Errors
	///
	/// Returns a [`PointerError`] if memory access failed.
	pub fn write_other_address_space(self, value: T) -> Result<(), PointerError> where T: Sized {
		// SAFETY: pointer derived from value so must be valid
		unsafe { self.copy_from_other_address_space(&raw const value, 1)? };
		core::mem::forget(value);
		Ok(())
	}

	/// Copies `count` elements starting at `start` into the buffer pointed to by `self`.
	///
	/// # Errors
	///
	/// Returns a [`PointerError`] if memory access failed.
	///
	/// # Safety
	///
	/// `src` must be [valid](`core::ptr#safety`) for reads of `count * size_of::<T>()` bytes or that number must be 0.
	///
	/// Like [`core::ptr::read`], `copy_from_other_address_space` creates a bitwise copy of `T`, regardless of
	/// whether `T` is [`Copy`]. If `T` is not [`Copy`], using both the values
	/// in the region beginning at `*start` and the region pointed to by `self` can
	/// [violate memory safety](`core::ptr::read#ownership-of-the-returned-value`).
	// FIXME(soundness): doesn't check if `self` points to conventional memory and therefore may write outside the page map region
	pub unsafe fn copy_from_other_address_space(self, start: *const T, count: usize) -> Result<(), PointerError> where T: Sized {
		let Some(address_space) = self.address_space else {
			return Err(PointerError {});
		};

		let dest_start = self.ptr.cast::<u8>();
		let mut chunk_start = dest_start;
		let end = self.ptr.wrapping_add(count).cast::<u8>();

		loop {
			let chunk_end = {
				let chunk_page_end = VirtualAddress::from(chunk_start).align_down_to_page() + 1usize;
				if chunk_page_end.addr >= end.addr() {
					end
				} else {
					chunk_page_end.as_ptr()
				}
			};

			let physical = crate::bridge::address_space::user::translate_addr(address_space, VirtualAddress::from(chunk_start))
					.ok_or(PointerError {})?;
			let physical = physical.to_virtual().as_ptr();

			// SAFETY: `chunk_start` and `chunk_end` both derived from `self.ptr`
			let chunk_size = unsafe { chunk_end.offset_from_unsigned(chunk_start) };

			// SAFETY: `chunk_start` and `dest_start` both derived from `self.ptr`
			let offset = unsafe { chunk_start.offset_from(dest_start) };

			// SAFETY: `offset` comes from `chunk_start` which is always between `start` and `start + count`, and
			// caller upholds requirement that allocation pointed to by `start` is `count` elements long.
			// `physical` is valid for writes as it comes from the page map region, and caller upholds
			// read requirements for pointer derived from `start`
			unsafe {
				core::ptr::copy_nonoverlapping(
					start.cast::<u8>().byte_offset(offset),
					physical,
					chunk_size,
				);
			}

			if chunk_end == end { break; }
			
			chunk_start = chunk_end;
		}

		Ok(())
	}

	/// Casts a pointer to another type.
	#[must_use = "this returns the result of the operation, without modifying the original"]
	pub const fn cast<U>(self) -> User<'a, *mut U> {
		User {
			ptr: self.ptr.cast(),
			address_space: self.address_space,
		}
	}

	/// Casts to a readable pointer.
	///
	/// # Safety
	///
	/// `addr`, if valid, must point to a valid bit pattern for `T` in the current address space.
	/// This means that in almost all cases, it is unsound to cast to a `User<*const T>` where `T`
	/// has a niche.
	///
	/// For all types with no invalid bit-patterns (i.e. all numeric types) it is sound to cast to
	/// a `User<*const T>`.
	#[must_use = "this returns the result of the operation, without modifying the original"]
	pub const unsafe fn cast_const(self) -> User<'a, *const T> {
		User {
			ptr: self.ptr.cast_const(),
			address_space: self.address_space,
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

impl<'a, T> User<'a, *const [T]> {
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
	pub const fn as_ptr(self) -> User<'a, *const T> {
		User {
			ptr: self.ptr.as_ptr(),
			address_space: self.address_space,
		}
	}

	/*
	pub fn read_to_box(self) -> Result<Box<[T]>, PointerError> {
		let mut buf = Box::new_uninit_slice(self.len());
		let size = self.read_to_buffer(&mut buf)?;
		assert_eq!(size, buf.len(), "box should be big enough");
		Ok(unsafe { buf.assume_init() })
	}
	
	pub fn read_to_buffer(self, buffer: &mut [MaybeUninit<T>]) -> Result<usize, PointerError> {
		assert!(
			crate::bridge::address_space::is_current(self.address_space),
			"Address space of User<*> should match current address space",
		);
		
		let count = min(buffer.len(), self.len());
		
		impls::checked_memcpy(
			self.ptr.cast(),
			buffer.as_mut_ptr().cast(),
			size_of::<T>() * count
		).ok_or(PointerError {})?;
		
		Ok(count)
	}*/

	/*
	pub fn read_to_buffer_other_address_space(self, buffer: &mut [MaybeUninit<T>]) -> Result<usize, PointerError> {
		todo!()
	}*/
}

impl<'a, T> User<'a, *mut [T]> {
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
	pub const fn as_mut_ptr(self) -> User<'a, *mut T> {
		User {
			ptr: self.ptr.as_mut_ptr(),
			address_space: self.address_space,
		}
	}

	/*
	pub fn write_from_slice(self, slice: &[T]) -> Result<usize, PointerError> {
		assert!(
			crate::bridge::address_space::is_current(self.address_space),
			"Address space of User<*> should match current address space",
		);

		let count = min(slice.len(), self.len());

		impls::checked_memcpy(
			slice.as_ptr().cast(),
			self.ptr.cast(),
			size_of::<T>() * count
		).ok_or(PointerError {})?;

		Ok(count)
	}
	

	pub fn write_from_slice_other_address_space(self, slice: &[T]) -> Result<usize, PointerError> {
		todo!()
	}*/
}

impl User<'_, *mut [u8]> {
	/// Fills `self` by repeating `value`.
	///
	/// # Errors
	///
	/// Returns a [`PointerError`] if memory access failed.
	pub fn fill(self, value: u8) -> Result<(), PointerError> {
		impls::checked_fill(
			value,
			self.ptr.cast(),
			self.len(),
		).ok_or(PointerError {})?;
		
		Ok(())
	}
}

/// Forms a [user pointer](`crate::ptr#user-pointers`) to a slice from a pointer and a length.
///
/// The `len` argument is the number of **elements**, not the number of bytes.
///
/// # Safety
///
/// If `data` points to accessible memory, it must point to `len` elements of type `T`.
/// See [`User::<*const T>::new`](`User::<*const T>::new#safety`) for more details.
#[must_use]
pub const unsafe fn slice_from_raw_parts<T>(data: User<'_, *const T>, len: usize) -> User<'_, *const [T]> {
	let ptr = core::ptr::slice_from_raw_parts(data.ptr, len);
	User {
		ptr,
		address_space: data.address_space,
	}
}

/// Forms a mutable [user pointer](`crate::ptr#user-pointers`) to a slice from a pointer and a length.
///
/// The `len` argument is the number of **elements**, not the number of bytes.
#[must_use]
pub const fn slice_from_raw_parts_mut<T>(data: User<'_, *mut T>, len: usize) -> User<'_, *mut [T]> {
	let ptr = core::ptr::slice_from_raw_parts_mut(data.ptr, len);
	User {
		ptr,
		address_space: data.address_space,
	}
}

impl<T: ?Sized> TryFrom<User<'_, *const T>> for LocalUser<*const T> {
	type Error = ();

	fn try_from(value: User<'_, *const T>) -> Result<Self, Self::Error> {
		if crate::bridge::address_space::is_current(value.address_space) {
			// SAFETY: checked address space for this pointer is the current address space,
			// and validity requirements of the address are the same for both `Self` and `User`
			Ok(unsafe { Self::new(value.ptr) })
		} else { Err(()) }
	}
}

impl<T: ?Sized> TryFrom<User<'_, *mut T>> for LocalUser<*mut T> {
	type Error = ();

	fn try_from(value: User<'_, *mut T>) -> Result<Self, Self::Error> {
		if crate::bridge::address_space::is_current(value.address_space) {
			Ok(Self::new(value.ptr))
		} else { Err(()) }
	}
}
