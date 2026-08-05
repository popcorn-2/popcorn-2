use core::fmt;
use core::marker::PhantomData;
use core::num::NonZero;
use core::ptr::slice_from_raw_parts;
use kernel_api::address_space::Kernel;
use kernel_api::mapping::{Config, Mapping, Mmap, Ty};

pub struct MappedVec<T> {
	storage: Option<Mapping<Mmap, Kernel>>,
	length: usize,
	_phantom: PhantomData<[T]>,
}

impl<T> MappedVec<T> {
	pub const fn new() -> Self {
		Self {
			storage: None,
			length: 0,
			_phantom: PhantomData
		}
	}

	pub fn push(&mut self, item: T) {
		if self.storage.is_none() {
			debug_assert!(self.capacity() == 0);
			debug_assert!(self.len() == 0);
			self.storage = Some(
				Config::new(NonZero::new(1).unwrap(), Ty::HEAP)
						.protection(true, false, false)
						.map()
						.expect("allocation failed")
			);
			debug_assert!(self.capacity() > self.len());
		}

		let storage = self.storage.as_mut().expect("storage must exist because capacity must be non-zero");

		// fixme(borrow views): replace with self.capacity()/len()
		if (storage.byte_len() / size_of::<T>()) < (self.length + 1) {
			storage.grow_in_place_by(storage.page_len()).expect("allocation expansion failed");
		}

		let ptr = storage
				.as_mut_ptr()
				.cast::<T>();

		debug_assert!(self.capacity() > self.len() + 1);

		// SAFETY: storage is valid for at least one more item to fit
		unsafe { ptr.add(self.len()).write(item) };
		self.length += 1;
	}

	pub fn capacity(&self) -> usize {
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
			items: unsafe { &*slice_from_raw_parts(ptr, self.len()) }
		}
	}
}

#[derive(Debug)]
pub struct IterMut<'vec, T> {
	_phantom: PhantomData<&'vec mut [T]>,
}

impl<'vec, T: 'vec> Iterator for IterMut<'vec, T> {
	type Item = &'vec mut T;

	fn next(&mut self) -> Option<Self::Item> {
		todo!()
	}
}

impl<'vec, T: 'vec> IntoIterator for &'vec mut MappedVec<T> {
	type Item = &'vec mut T;
	type IntoIter = IterMut<'vec, T>;

	fn into_iter(self) -> Self::IntoIter {
		todo!()
	}
}
