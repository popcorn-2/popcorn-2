use core::marker::PhantomData;
use core::num::NonZero;
use core::ptr::slice_from_raw_parts;
use kernel_api::memory::mapping::{Config, Mapping, new_mapping};

pub struct MappedVec<T> {
	storage: Option<Mapping<'static>>,
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
			self.storage = Some(new_mapping(Config::new(NonZero::new(1).unwrap()), 25).expect("allocation failed"));
			debug_assert!(self.capacity() > self.len());
		}

		let storage = self.storage.as_mut().expect("storage must exist because capacity must be non-zero");

		// fixme(borrow views): replace with self.capacity()/len()
		if (storage.physical_len().get() * 4096 / size_of::<T>()) < (self.length + 1) {
			storage.resize_in_place(
				storage.physical_len().checked_mul(
					NonZero::new(2).unwrap()
				).expect("allocation overflowed")
			).expect("allocation expansion failed");
		}

		let ptr = storage
				.virtual_start().as_ptr()
				.cast::<T>();

		debug_assert!(self.capacity() > self.len() + 1);

		// SAFETY: storage is valid for at least one more item to fit
		unsafe { ptr.add(self.len()).write(item); }
		self.length += 1;
	}

	pub fn capacity(&self) -> usize {
		let Some(storage) = &self.storage else { return 0; };
		storage.physical_len().get() * 4096 / size_of::<T>()
	}

	pub fn len(&self) -> usize {
		self.length
	}
}

impl<T> Drop for MappedVec<T> {
	fn drop(&mut self) {
		for val in self {
			unsafe { core::ptr::drop_in_place(val); }
		}
	}
}

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
				.map(|map| map.virtual_start().as_ptr().cast_const().cast())
				.unwrap_or(core::ptr::dangling());
		Iter {
			items: unsafe { &*slice_from_raw_parts(ptr, self.len()) }
		}
	}
}

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
