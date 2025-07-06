use core::ptr;
use core::sync::atomic::{AtomicPtr, Ordering};
use crate::threading::{CoreId, ThreadControlBlock, ThreadPointer};

/// Effectively the following enum, but compressed using pointer tagging to allow it to fit within
/// a `usize` and therefore be atomic
///
/// ```ignore
/// enum PointerState {
///     GloballyParked(ThreadPointer), // which is just a wrapper around NonNull<ThreadControlBlock>
///     InScheduler(CoreId),
///     UnparkInProgress,
/// }
/// ```
///
/// [`ThreadControlBlock`]s will always be stored in the kernel heap, and therefore have a higher half address
/// with the MSB set to 1.
/// [`CoreId`]s are a `usize` but limited to a maximum range of `isize::MAX`, and therefore will never have the MSB set to 1.
/// This means that the MSB of the internally stored pointer can be used to differentiate between the `GloballyParked` and
/// `InScheduler` states.
/// Additionally, [`ThreadControlBlock`]s are aligned to at least 2 bytes, meaning a valid pointer to a [`ThreadControlBlock`]
/// will always have the LSB set to 0.
/// This means that by setting the MSB and LSB to 1, we get free use of all other bits to hold other non-payloaded values.
///
/// In this case, we only have `UnparkInProgress`, which is stored as all zeroes except for the MSB and LSB being 1.
pub struct AtomicPointerState(AtomicPtr<ThreadControlBlock>);

impl AtomicPointerState {
	const UNPARK_VARIANT: usize = 0;

	pub fn new(state: PointerState<ThreadPointer>) -> Self {
		match state {
			PointerState::GloballyParked(ptr) => {
				let ptr = ptr.to_raw_ptr().as_ptr();
				assert_ne!(ptr.addr() & (1<<63), 0, "TCB pointer should always be in higher half of address space");
				assert_eq!(ptr.addr() & 1, 0, "TCB pointer should always be aligned to 2 bytes");
				Self(AtomicPtr::new(ptr))
			},
			PointerState::InScheduler(core_id) => {
				let core_id = core_id.id as usize;
				assert_eq!(core_id & (1 << 63), 0, "CoreId should never be >isize::MAX");
				Self(AtomicPtr::new(ptr::without_provenance_mut(core_id)))
			},
			PointerState::UnparkInProgress => {
				Self(AtomicPtr::new(ptr::without_provenance_mut(
					(Self::UNPARK_VARIANT << 1) | (1 << 63) | 1
				)))
			},
		}
	}
	
	pub fn load(&self, ordering: Ordering) -> PointerState<()> {
		let state = self.0.load(ordering);
		let msb = state.addr() & (1 << 63) != 0;
		let lsb = state.addr() & (1 << 0) != 0;
		
		match (msb, lsb) {
			(false, _) => PointerState::InScheduler(CoreId { id: state.addr() as isize }),
			(true, false) => PointerState::GloballyParked(()),
			(true, true) => match (state.addr() & !(1 << 63)) >> 1 {
				Self::UNPARK_VARIANT => PointerState::UnparkInProgress,
				_ => unreachable!("AtomicPointerState contained invalid value"),
			}
		}
	}
}

pub enum PointerState<T> {
	GloballyParked(T),
	InScheduler(CoreId),
	UnparkInProgress,
}
