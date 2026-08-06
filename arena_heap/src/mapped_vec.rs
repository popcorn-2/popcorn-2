use core::{fmt, slice};
use core::marker::PhantomData;
use core::num::NonZero;
use kernel_api::address_space::Kernel;
use kernel_api::mapping::{Config, Mapping, Mmap, Ty};
use kernel_api::memory::PAGE_SIZE;

// INVARIANT: `Mapping` contains `length` initialised elements of `T`
pub struct MappedVec<T> {
	storage: Option<Mapping<Mmap, Kernel>>,
	length: usize,
	_phantom: PhantomData<[T]>,
}

impl<T> MappedVec<T> {
	pub const fn new() -> Self {
		const { assert!(PAGE_SIZE >= align_of::<T>(), "elements of MappedVec must have lower alignment than PAGE_SIZE") };
		Self {
			storage: None,
			length: 0,
			_phantom: PhantomData
		}
	}

	/// # Panics
	///
	/// If allocation for the new item fails.
	pub fn push(&mut self, item: T) {
		let storage = self.storage.get_or_insert_with(|| {
			Config::new(NonZero::new(1).unwrap(), Ty::HEAP)
				.protection(true, false, false)
				.map()
				.expect("allocation failed")
		});

		// fixme(borrow views): replace with self.capacity()/len()
		if (storage.byte_len() / size_of::<T>()) < (self.length + 1) {
			storage.grow_in_place_by(storage.page_len()).expect("allocation expansion failed");
		}

		let ptr = storage
				.as_mut_ptr()
				.cast::<T>();

		debug_assert!(self.capacity() > self.len(), "capacity did not grow after expansion");

		// SAFETY: storage is valid for at least one more item to fit
		unsafe { ptr.add(self.len()).write(item) };
		self.length += 1;
	}

	pub const fn capacity(&self) -> usize {
		let Some(storage) = &self.storage else { return 0; };
		storage.byte_len() / size_of::<T>()
	}

	pub const fn len(&self) -> usize {
		self.length
	}
}

impl<T: fmt::Debug> fmt::Debug for MappedVec<T> {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		f.debug_list().entries(self).finish()
	}
}

impl<T> Drop for MappedVec<T> {
	fn drop(&mut self) {
		for val in self {
			// SAFETY: `val` comes from a mut reference
			unsafe { core::ptr::drop_in_place(val); }
		}
	}
}

#[derive(Debug)]
pub struct Iter<'vec, T> {
	items: &'vec [T],
}

impl<'vec, T: 'vec> Iterator for Iter<'vec, T> {
	type Item = &'vec T;

	fn next(&mut self) -> Option<Self::Item> {
		let (head, rest) = self.items.split_first()?;
		self.items = rest;
		Some(head)
	}
}

impl<'vec, T: 'vec> IntoIterator for &'vec MappedVec<T> {
	type Item = &'vec T;
	type IntoIter = Iter<'vec, T>;

	fn into_iter(self) -> Self::IntoIter {
		let ptr = self.storage.as_ref()
				.map_or(core::ptr::dangling(), |map| map.as_ptr().cast());
		Iter {
			// SAFETY: Invariant of `MappedVec` that `ptr` points to `len()` initialised elements
			//  of type `T`
			items: unsafe { slice::from_raw_parts(ptr, self.len()) }
		}
	}
}

#[derive(Debug)]
pub struct IterMut<'vec, T> {
	items: Option<&'vec mut [T]>,
}

impl<'vec, T: 'vec> Iterator for IterMut<'vec, T> {
	type Item = &'vec mut T;

	fn next(&mut self) -> Option<Self::Item> {
		let items: &'vec mut _ = self.items.take().expect("corrupted IterMut");
		let (head, rest) = items.split_first_mut()?;
		self.items = Some(rest);
		Some(head)
	}
}

impl<'vec, T: 'vec> IntoIterator for &'vec mut MappedVec<T> {
	type Item = &'vec mut T;
	type IntoIter = IterMut<'vec, T>;

	fn into_iter(self) -> Self::IntoIter {
		let ptr = self.storage.as_mut()
			.map_or(core::ptr::dangling_mut(), |map| map.as_mut_ptr().cast());
		IterMut {
			// SAFETY: Invariant of `MappedVec` that `ptr` points to `len()` initialised elements
			//  of type `T`
			items: Some(unsafe { slice::from_raw_parts_mut(ptr, self.len()) })
		}
	}
}
