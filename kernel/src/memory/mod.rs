pub mod r#virtual;
pub mod physical;
pub mod paging;
pub mod watermark_allocator;

#[cfg(test)]
mod tests {
	use core::num::NonZeroUsize;
	use core::sync::atomic::{AtomicUsize, Ordering};
	use kernel_api::memory::allocator::BackingAllocator;
	use kernel_api::memory::{AllocError, Frame, PhysicalAddress};

	struct MockAllocator {
		expected_frames: usize,
		actual_frames: AtomicUsize,
		base: Frame,
		always_fail: bool
	}

	impl MockAllocator {
		fn new_fail() -> Self {
			Self {
				always_fail: true,
				.. Default::default()
			}
		}

		fn new_with_frames(expected: usize) -> Self {
			Self {
				expected_frames: expected,
				.. Default::default()
			}
		}

		fn verify(&self) {
			let actual = self.actual_frames.load(Ordering::Acquire);
			if actual != self.expected_frames {
				panic!("{actual} frames allocated when {} were expected", self.expected_frames);
			}
		}
	}

	impl Default for MockAllocator {
		fn default() -> Self {
			Self {
				expected_frames: Default::default(),
				actual_frames: Default::default(),
				base: Frame::new(PhysicalAddress::new(0)),
				always_fail: false
			}
		}
	}

	unsafe impl BackingAllocator for MockAllocator {
		fn allocate_contiguous(&self, frame_count: usize) -> Result<Frame, AllocError> {
			if self.always_fail { return Err(AllocError); }

			let old_frame_count = self.actual_frames.fetch_add(frame_count, Ordering::Release);
			if (old_frame_count + frame_count) > self.expected_frames {
				panic!("Attempted to allocate {} frames when only {} were expected", old_frame_count + frame_count, self.expected_frames);
			}

			Ok(self.base + old_frame_count)
		}

		unsafe fn deallocate_contiguous(&self, _: Frame, _: NonZeroUsize) {}
	}

	#[test]
	fn mock_sanity() {
		let mock = MockAllocator::new_fail();
		assert!(mock.allocate_contiguous(4).is_err());

		let mock = MockAllocator::new_with_frames(5);
		assert!(mock.allocate_contiguous(3).is_ok());
		assert!(mock.allocate_contiguous(2).is_ok());
		mock.verify();
	}
/*
	#[test_case]
	fn allocate_returns_correct_number() {
		let mock = MockAllocator::new_with_frames(5);
		let allocation = mock.allocate_contiguous(5);
		let len = allocation
			.inspect(|a| assert!(a.is_ok()))
			.count();
		assert_eq!(len, 5);
		mock.verify();
	}*/

	/*#[test_case]
	fn allocations_do_not_overlap() {
		let mut allocation_store = MaybeUninit::uninit_array::<5>();
		let mock = MockAllocator::new_with_frames(5);
		let allocations = mock.allocate_contiguous(5);
		for (i, allocation) in allocations.enumerate() {
			assert!(allocation.is_ok());
			allocation_store[i].write(allocation.unwrap());
		}
		mock.verify();

		let allocation_store = unsafe { MaybeUninit::array_assume_init(allocation_store) };
		for (i, frame) in allocation_store.iter().enumerate() {
			assert!(!allocation_store[..i].contains(frame));
		}
	}*/

	#[test]
	fn allocate_one_allocates_one() {
		let mock = MockAllocator::new_with_frames(1);
		assert!(mock.allocate_one().is_ok());
		mock.verify();
	}
}
