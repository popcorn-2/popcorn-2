use core::cell::UnsafeCell;
use core::fmt;
use core::mem::MaybeUninit;
use core::ops::Deref;
use core::sync::atomic::{AtomicU8, fence, Ordering};

/// A low-level synchronization primitive for one-time global execution.
///
/// # Examples
///
/// ```
/// use kernel_api::sync::Once;
///
/// static START: Once = Once::new();
///
/// START.call_once(|| {
///     // run initialization here
/// });
/// ```
pub struct Once(AtomicU8);

impl fmt::Debug for Once {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		f.debug_tuple("Once")
			.field(&State::try_from(self.0.load(Ordering::SeqCst)).unwrap())
			.finish()
	}
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
#[repr(u8)]
enum State {
    Uncalled = 0,
    Running = 1,
    Called = 2,
    Poison = 3
}

impl State {
    const fn const_into_u8(self) -> u8 {
        match self {
            State::Uncalled => 0,
            State::Running => 1,
            State::Called => 2,
            State::Poison => 3
        }
    }

    const fn const_from_u8(value: u8) -> Result<Self, ()> {
        match value {
            0 => Ok(State::Uncalled),
            1 => Ok(State::Running),
            2 => Ok(State::Called),
            3 => Ok(State::Poison),
            _ => Err(())
        }
    }
}

impl From<State> for u8 {
    fn from(value: State) -> Self {
        value.const_into_u8()
    }
}

impl TryFrom<u8> for State {
    type Error = ();

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        Self::const_from_u8(value)
    }
}

impl Once {
	/// Creates a new `Once` value.
	#[must_use]
    pub const fn new() -> Self {
        Self(AtomicU8::new(State::Uncalled.const_into_u8()))
    }

	/// Performs an initialization routine once and only once. The given closure
	/// will be executed if this is the first time `call_once` has been called,
	/// and otherwise the routine will *not* be invoked.
	///
	/// This method will spin if another initialization
	/// routine is currently running.
	///
	/// When this function returns, it is guaranteed that some initialization
	/// has run and completed (it might not be the closure specified). It is also
	/// guaranteed that any memory writes performed by the executed closure can
	/// be reliably observed by other threads at this point (there is a
	/// happens-before relation between the closure and code executing after the
	/// return).
	///
	/// If the given closure recursively invokes `call_once` on the same [`Once`]
	/// instance, the exact behavior is not specified: allowed outcomes are
	/// a panic or a deadlock.
	///
	/// # Examples
	///
	/// ```
	/// use kernel_api::sync::Once;
	///
	/// static mut VAL: usize = 0;
	/// static INIT: Once = Once::new();
	///
	/// // Accessing a `static mut` is unsafe much of the time, but if we do so
	/// // in a synchronized fashion (e.g., write once or read all) then we're
	/// // good to go!
	/// //
	/// // This function will only call `expensive_computation` once, and will
	/// // otherwise always return the value returned from the first invocation.
	/// fn get_cached_val() -> usize {
	///     unsafe {
	///         INIT.call_once(|| {
	///             VAL = expensive_computation();
	///         });
	///         VAL
	///     }
	/// }
	///
	/// fn expensive_computation() -> usize {
	///     // ...
	/// # 2
	/// }
	/// ```
	///
	/// # Panics
	///
	/// The closure `f` will only be executed once even if this is called
	/// concurrently amongst many threads. If that closure panics, however, then
	/// it will *poison* this [`Once`] instance, causing all future invocations of
	/// `call_once` to also panic.
    pub fn call_once<F: FnOnce()>(&self, f: F) {
        loop {
            let current = self.0.compare_exchange_weak(State::Uncalled.into(), State::Running.into(), Ordering::Relaxed, Ordering::Acquire);
            match current {
                Ok(_) => break, // Switched from Uncalled to Running, call the function
                Err(s) if s == State::Poison.into() => panic!("poisoned `Once`"),
                Err(s) if s == State::Running.into() => {}, // Currently running, spin until state changes
                Err(s) if s == State::Called.into() => return, // Already called, return immediately
                Err(s) if s == State::Uncalled.into() => {}, // weak cas fail, try again
                _ => unreachable!()
            }
            core::hint::spin_loop();
        }

        struct DropGuard<'a>(&'a Once);
        impl Drop for DropGuard<'_> {
            fn drop(&mut self) {
                self.0.0.store(State::Poison.into(), Ordering::Relaxed);
            }
        }
        let drop_guard = DropGuard(self);

        f();

        core::mem::forget(drop_guard);

        self.0.store(State::Called.into(), Ordering::Release);
    }

	/// Returns `true` if some [`call_once()`] call has completed
	/// successfully. Specifically, `is_completed` will return false in
	/// the following situations:
	///   * [`call_once()`] was not called at all,
	///   * [`call_once()`] was called, but has not yet completed,
	///   * the [`Once`] instance is poisoned
	///
	/// This function returning `false` does not mean that [`Once`] has not been
	/// executed. For example, it may have been executed in the time between
	/// when `is_completed` starts executing and when it returns, in which case
	/// the `false` return value would be stale (but still permissible).
	///
	/// # Panics
	///
	/// If this `Once` is poisoned due to the closure passed to [`call_once`] panicking.
	///
	/// # Examples
	///
	/// ```
	/// use kernel_api::sync::Once;
	///
	/// static INIT: Once = Once::new();
	///
	/// assert_eq!(INIT.is_completed(), false);
	/// INIT.call_once(|| {
	///     assert_eq!(INIT.is_completed(), false);
	/// });
	/// assert_eq!(INIT.is_completed(), true);
	/// ```
	#[must_use]
	pub fn is_complete(&self) -> bool {
        let state = self.0.load(Ordering::Relaxed).try_into().unwrap();

        matches!(state, State::Called)
    }
}

impl const Default for Once {
    fn default() -> Self {
        Self::new()
    }
}

/// A synchronization primitive which can nominally be written to only once.
///
/// This type is a thread-safe [`OnceCell`](`core::cell::OnceCell`), and can be used in statics.
/// In many simple cases, you can use [`LazyLock<T, F>`] instead to get the benefits of this type
/// with less effort: `LazyLock<T, F>` "looks like" `&T` because it initializes with `F` on deref!
/// Where `OnceLock` shines is when `LazyLock` is too simple to support a given case, as `LazyLock`
/// doesn't allow additional inputs to its function after you call [`LazyLock::new(|| ...)`](`LazyLock::new`).
///
/// A `OnceLock` can be thought of as a safe abstraction over uninitialized data that becomes
/// initialized once written.
/*///
/// # Examples
///
/// Writing to a `OnceLock` from a separate thread:
///
/// ```
/// use kernel_api::sync::OnceLock;
///
/// static CELL: OnceLock<usize> = OnceLock::new();
///
/// // `OnceLock` has not been written to yet.
/// assert!(CELL.get().is_none());
///
/// // Spawn a thread and write to `OnceLock`.
/// std::thread::spawn(|| {
///     let value = CELL.get_or_init(|| 12345);
///     assert_eq!(value, &12345);
/// })
/// .join()
/// .unwrap();
///
/// // `OnceLock` now contains the value.
/// assert_eq!(
///     CELL.get(),
///     Some(&12345),
/// );
/// ```*/
#[derive(Debug)]
pub struct OnceLock<T> {
    data: UnsafeCell<MaybeUninit<T>>,
    once: Once
}

// SAFETY: `T` is `Send` therefore exposing it to other threads is sound
unsafe impl<T: Send> Send for OnceLock<T> {}
// SAFETY: `T` is `Sync` therefore exposing it to other threads via ref is sound
unsafe impl<T: Send + Sync> Sync for OnceLock<T> {}

impl<T> OnceLock<T> {
	/// Creates a new uninitialized cell.
	#[must_use]
	pub const fn new() -> Self {
        Self {
            data: UnsafeCell::new(MaybeUninit::uninit()),
            once: Once::new()
        }
    }

	/// Gets the reference to the underlying value.
	///
	/// Returns `None` if the cell is uninitialized, or being initialized.
	/// This method never blocks.
    #[inline]
    pub fn get(&self) -> Option<&T> {
        if !self.once.is_complete() { return None; }
        fence(Ordering::Acquire);

		// SAFETY: we only expose a shared ref so mutation cannot occur
		let ret = unsafe { &*self.data.get() };
		// SAFETY: checked above that the OnceLock is initialised
        unsafe { Some(ret.assume_init_ref()) }
    }

	/// Gets the mutable reference to the underlying value.
	///
	/// Returns `None` if the cell is uninitialized.
	///
	/// This method never blocks. Since it borrows the `OnceLock` mutably,
	/// it is statically guaranteed that no active borrows to the `OnceLock`
	/// exist, including from other threads.
    #[inline]
    pub fn get_mut(&mut self) -> Option<&mut T> {
        if !self.once.is_complete() { return None; }
        fence(Ordering::Acquire);

		// SAFETY: checked that OnceLock has been initialized
        unsafe {
            Some(self.data.get_mut().assume_init_mut())
        }
    }

	/// Gets the contents of the cell, initializing it to `f()` if the cell
	/// was uninitialized.
	///
	/// Many threads may call `get_or_init` concurrently with different
	/// initializing functions, but it is guaranteed that only one function
	/// will be executed if the function doesn't panic.
	///
	/// # Panics
	///
	/// If `f()` panics, the panic is propagated to the caller, and the cell
	/// remains uninitialized.
	///
	/// It is an error to reentrantly initialize the cell from `f`. The
	/// exact outcome is unspecified.
	///
	/// # Examples
	///
	/// ```
	/// use kernel_api::sync::OnceLock;
	///
	/// let cell = OnceLock::new();
	/// let value = cell.get_or_init(|| 92);
	/// assert_eq!(value, &92);
	/// let value = cell.get_or_init(|| unreachable!());
	/// assert_eq!(value, &92);
	/// ```
    #[inline]
    pub fn get_or_init(&self, f: impl FnOnce() -> T) -> &T {
		// SAFETY: `call_once` ensures that the closure only runs once so the pointer cannot be aliased
        self.once.call_once(|| unsafe { (*self.data.get()).write(f()); });
		// SAFETY: checked that OnceLock has been initialized
        unsafe { (*self.data.get()).assume_init_ref() }
    }
}

impl<T> const Default for OnceLock<T> {
	fn default() -> Self {
		Self::new()
	}
}

/// A value which is initialized on the first access.
///
/// This type is a thread-safe [`LazyCell`](`core::cell::LazyCell`), and can be used in statics.
/// Since initialization may be called from multiple threads, any
/// dereferencing call will block the calling thread if another
/// initialization routine is currently running.
///
/// # Poisoning
///
/// If the initialization closure passed to [`LazyLock::new`] panics, the lock will be poisoned.
/// Once the lock is poisoned, any threads that attempt to access this lock (via a dereference
/// or via an explicit call to [`force()`](`LazyLock::force`)) will panic.
///
/// # Examples
///
/// Initialize static variables with `LazyLock`.
/// ```
/// use kernel_api::sync::LazyLock;
///
/// // Note: static items do not call [`Drop`] on program termination, so this won't be deallocated.
/// static DEEP_THOUGHT: LazyLock<String> = LazyLock::new(|| {
/// # mod another_crate {
/// #     pub fn great_question() -> String { "42".to_string() }
/// # }
///     another_crate::great_question()
/// });
///
/// // The `String` is built, stored in the `LazyLock`, and returned as `&String`.
/// let _ = &*DEEP_THOUGHT;
/// ```
///
/// Initialize fields with `LazyLock`.
/// ```
/// use kernel_api::sync::LazyLock;
///
/// #[derive(Debug)]
/// struct UseCellLock {
///     number: LazyLock<u32>,
/// }
/// fn main() {
///     let lock: LazyLock<u32> = LazyLock::new(|| 0u32);
///
///     let data = UseCellLock { number: lock };
///     assert_eq!(*data.number, 0);
/// }
/// ```
#[derive(Debug)]
pub struct LazyLock<T, F = fn() -> T> {
    once: OnceLock<T>,
    // FIXME: actually drop this when needed
    f: MaybeUninit<F>
}

// SAFETY: Only move out of F and never create an &F so don't need f: Sync
//  LazyLock exposes &T from &LazyLock so T must be Sync + Send for LazyLock to be Sync
unsafe impl<T: Sync + Send, F: Send> Sync for LazyLock<T, F> {}

impl<T, F: FnOnce() -> T> LazyLock<T, F> {
	/// Creates a new lazy value with the given initializing function.
	///
	/// # Examples
	///
	/// ```
	/// use kernel_api::sync::LazyLock;
	///
	/// let hello = "Hello, World!".to_string();
	///
	/// let lazy = LazyLock::new(|| hello.to_uppercase());
	///
	/// assert_eq!(&*lazy, "HELLO, WORLD!");
	/// ```
	#[must_use]
	pub const fn new(f: F) -> Self {
        Self {
            once: OnceLock::new(),
            f: MaybeUninit::new(f)
        }
    }

	/// Forces the evaluation of this lazy value and returns a reference to
	/// result. This is equivalent to the `Deref` impl, but is explicit.
	///
	/// This method will block the calling thread if another initialization
	/// routine is currently running.
	///
	/// # Panics
	///
	/// If the initialization closure panics (the one that is passed to the [`new()`](`LazyLock::new`) method), the
	/// panic is propagated to the caller, and the lock becomes poisoned. This will cause all future
	/// accesses of the lock (via [`force()`](`LazyLock::force`) or a dereference) to panic.
	///
	/// # Examples
	///
	/// ```
	/// use kernel_api::sync::LazyLock;
	///
	/// let lazy = LazyLock::new(|| 92);
	///
	/// assert_eq!(LazyLock::force(&lazy), &92);
	/// assert_eq!(&*lazy, &92);
	/// ```
    pub fn force(this: &Self) -> &T {
		// SAFETY: `this.once` ensures `this.f` is only moved out of once
        this.once.get_or_init(unsafe {
            core::ptr::read(this.f.as_ptr())
        })
    }
}

impl<T, F: FnOnce() -> T> Deref for LazyLock<T, F> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        Self::force(self)
    }
}
