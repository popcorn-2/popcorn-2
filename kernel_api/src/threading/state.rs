use core::fmt::{Debug, Formatter};
use core::sync::atomic::{Ordering, AtomicU128};

const fn into_raw(state: ThreadState) -> u128 {
	// SAFETY: pointer is derived from a reference to a `repr(u8)` enum
	let tag = u128::from(unsafe { *(&raw const state).cast::<u8>() });

	match state {
		ThreadState::Killed(exit_code) => {
			let upper = (exit_code as i128).cast_unsigned();
			(upper << 64) | tag
		}
		_ => tag,
	}
}

fn from_raw(raw: u128) -> ThreadState {
	let tag = raw.truncate::<u8>();

	match tag {
		0 => ThreadState::Ready,
		1 => ThreadState::Running,
		2 => ThreadState::Parked,
		3 => ThreadState::NearlyParked,
		4 => ThreadState::Killed((raw >> 64).truncate::<usize>().cast_signed()),
		_ => unreachable!("invalid thread state"),
	}
}

/// An atomic version of [`ThreadState`].
pub struct AtomicThreadState(AtomicU128);

impl AtomicThreadState {
	/// Creates a new `AtomicThreadState` from the given [`ThreadState`].
	///
	/// # Examples
	///
	/// ```
	/// use kernel_api::threading::{ThreadState, AtomicThreadState};
	///
	/// let state = AtomicThreadState::new(ThreadState::Running);
	/// assert!(state.running());
	/// ```
	#[must_use]
	pub const fn new(state: ThreadState) -> Self {
		Self(AtomicU128::new(into_raw(state)))
	}

	/// Atomically updates `self` to `state`.
	///
	/// `store` takes an [`Ordering`] argument which describes the memory ordering of this operation.
	/// Possible values are [`SeqCst`](`Ordering::SeqCst`), [`Release`](`Ordering::Release`) and [`Relaxed`](`Ordering::Relaxed`).
	///
	/// # Panics
	///
	/// Panics if `ordering` is [`Acquire`](`Ordering::Acquire`) or [`AcqRel`](`Ordering::AcqRel`).
	///
	/// # Examples
	///
	/// ```
	/// use kernel_api::threading::{ThreadState, AtomicThreadState};
	/// use core::sync::atomic::Ordering;
	///
	/// let state = AtomicThreadState::new(ThreadState::Running);
	/// assert!(state.running());
	/// state.store(ThreadState::Killed(-1), Ordering::SeqCst);
	/// assert!(!state.running());
	/// ```
	pub fn store(&self, state: ThreadState, ordering: Ordering) {
		self.0.store(into_raw(state), ordering);
	}

	/// Atomically retrieves the current state of `self`.
	///
	/// `load` takes an [`Ordering`] argument which describes the memory ordering of this operation.
	/// Possible values are [`SeqCst`](`Ordering::SeqCst`), [`Acquire`](`Ordering::Acquire`) and [`Relaxed`](`Ordering::Relaxed`).
	///
	/// # Panics
	///
	/// Panics if `ordering` is [`Release`](`Ordering::Release`) or [`AcqRel`](`Ordering::AcqRel`).
	///
	/// # Examples
	///
	/// ```
	/// use kernel_api::threading::{ThreadState, AtomicThreadState};
	/// use core::sync::atomic::Ordering;
	/// use core::assert_matches;
	///
	/// let state = AtomicThreadState::new(ThreadState::Running);
	/// let state = state.load(Ordering::SeqCst);
	/// assert_matches!(state, ThreadState::Running);
	/// ```
	pub fn load(&self, ordering: Ordering) -> ThreadState{
		from_raw(self.0.load(ordering))
	}

	/// Stores a value into the thread state if the current state is the same as
	/// the `current` state.
	///
	/// The return value is a result indicating whether the new state was written and
	/// containing the previous state. On success this value is guaranteed to be equal to
	/// `current`.
	///
	/// `compare_exchange` takes two [`Ordering`] arguments to describe the memory
	/// ordering of this operation. `success` describes the required ordering for the
	/// read-modify-write operation that takes place if the comparison with `current` succeeds.
	/// `failure` describes the required ordering for the load operation that takes place when
	/// the comparison fails. Using [`Acquire`](`Ordering::Acquire`) as success ordering makes the store part
	/// of this operation [`Relaxed`](`Ordering::Relaxed`), and using [`Release`](`Ordering::Release`) makes the successful load
	/// [`Relaxed`](`Ordering::Relaxed`). The failure ordering can only be [`SeqCst`](`Ordering::SeqCst`), [`Acquire`](`Ordering::Acquire`) or [`Relaxed`](`Ordering::Relaxed`).
	///
	/// # Examples
	///
	/// ```
	/// use kernel_api::threading::{ThreadState, AtomicThreadState};
	/// use core::sync::atomic::Ordering;
	///
	/// let state = AtomicThreadState::new(ThreadState::Killed(0));
	///
	/// assert_eq!(state.compare_exchange(ThreadState::Killed(0), ThreadState::Running,
	///                                      Ordering::Acquire,
	///                                      Ordering::Relaxed),
	///            Ok(ThreadState::Killed(0)));
	/// assert_eq!(some_var.load(Ordering::Relaxed), ThreadState::Running);
	///
	/// assert_eq!(some_var.compare_exchange(ThreadState::Runnable, ThreadState::Parked,
	///                                      Ordering::SeqCst,
	///                                      Ordering::Acquire),
	///            Err(ThreadState::Running));
	/// assert_eq!(some_var.load(Ordering::Relaxed), ThreadState::Running);
	/// ```
	#[expect(clippy::missing_errors_doc, reason = "doesn't make sense in this context")]
	pub fn compare_exchange(&self, current: ThreadState, new: ThreadState, success: Ordering, failure: Ordering) -> Result<ThreadState, ThreadState> {
		let current = into_raw(current);
		let new = into_raw(new);
		self.0.compare_exchange(current, new, success, failure)
				.map(from_raw)
				.map_err(from_raw)
	}

	/// Fetches the current state, and applies a function to it that returns an optional
	/// new state. Returns a `Result` of `Ok(previous_state)` if the function returned `Some(_)`, else
	/// `Err(previous_state)`.
	///
	/// Note: This may call the function multiple times if the value has been changed from other threads in
	/// the meantime, as long as the function returns `Some(_)`, but the function will have been applied
	/// only once to the stored value.
	///
	/// `fetch_update` takes two [`Ordering`] arguments to describe the memory ordering of this operation.
	/// The first describes the required ordering for when the operation finally succeeds while the second
	/// describes the required ordering for loads. These correspond to the success and failure orderings of
	/// [`compare_exchange`](Self::compare_exchange) respectively.
	///
	/// Using [`Acquire`](`Ordering::Acquire`) as success ordering makes the store part
	/// of this operation [`Relaxed`](`Ordering::Relaxed`), and using [`Release`](`Ordering::Release`) makes the final successful load
	/// [`Relaxed`](`Ordering::Relaxed`). The (failed) load ordering can only be [`SeqCst`](`Ordering::SeqCst`), [`Acquire`](`Ordering::Acquire`) or [`Relaxed`](`Ordering::Relaxed`).
	///
	/// # Examples
	///
	/// ```rust
	/// use kernel_api::threading::{ThreadState, AtomicThreadState};
	/// use core::sync::atomic::Ordering;
	///
	/// let state = AtomicThreadState::new(ThreadState::Killed(0));
	/// assert_eq!(state.fetch_update(Ordering::SeqCst, Ordering::SeqCst, |_| None), Err(ThreadState::Running));
	/// assert_eq!(
	///     state.fetch_update(
	///         Ordering::SeqCst,
	///         Ordering::SeqCst,
	///         |old_state| {
	///             match old_state {
	///                 ThreadState::Killed(code) => Some(ThreadState::Killed(code + 1)),
	///                 _ => None,
	///             }
	///         }
	///     ),
	///     Ok(ThreadState::Killed(0))
	/// );
	/// assert_eq!(state.load(Ordering::SeqCst), ThreadState::Killed(1));
	/// ```
	#[expect(clippy::missing_errors_doc, reason = "doesn't make sense in this context")]
	pub fn fetch_update(&self, set_order: Ordering, fetch_order: Ordering, mut f: impl FnMut(ThreadState) -> Option<ThreadState>) -> Result<ThreadState, ThreadState> {
		self.0.fetch_update(set_order, fetch_order, |val| f(from_raw(val)).map(into_raw))
				.map(from_raw)
				.map_err(from_raw)
	}
	
	/// Returns `true` if the thread is actively running.
	///
	/// # Examples
	///
	/// ```
	/// use kernel_api::threading::{ThreadState, AtomicThreadState};
	///
	/// let running = AtomicThreadState::new(ThreadState::Running);
	/// assert!(running.running());
	///
	/// let dead = AtomicThreadState::new(ThreadState::Killed(0));
	/// assert!(!dead.running());
	/// ```
	pub fn running(&self) -> bool {
		matches!(
			self.load(Ordering::SeqCst),
			ThreadState::Running,
		)
	}

	/// Returns `true` if the thread could be run in its current state.
	///
	/// # Examples
	///
	/// ```
	/// use kernel_api::threading::{ThreadState, AtomicThreadState};
	///
	/// let runnable = AtomicThreadState::new(ThreadState::Runnable);
	/// assert!(runnable.runnable());
	///
	/// let dead = AtomicThreadState::new(ThreadState::Killed(0));
	/// assert!(!dead.runnable());
	/// ```
	pub fn runnable(&self) -> bool {
		matches!(
			self.load(Ordering::SeqCst),
			ThreadState::Running | ThreadState::Ready | ThreadState::NearlyParked,
		)
	}
}

impl Debug for AtomicThreadState {
	fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
		write!(f, "AtomicThreadState {{ {:?} }}", self.load(Ordering::Relaxed))
	}
}

/// The current state of a thread.
#[derive(Debug, Copy)]
#[derive_const(Clone)]
#[repr(u8)]
pub enum ThreadState {
	/// The thread is able to run, but has not yet been scheduled.
	Ready = 0,
	/// The thread is actively running.
	Running = 1,
	/// The thread is parked.
	Parked = 2,
	/// The thread is in the process of being parked.
	NearlyParked = 3,
	/// The thread has exited with a return code.
	Killed(isize) = 4,
}
