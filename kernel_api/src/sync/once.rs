#![unstable(feature = "kernel_sync_once", issue = "none")]

use core::cell::UnsafeCell;
use core::mem::MaybeUninit;
use core::ops::Deref;
use core::sync::atomic::{AtomicU8, fence, Ordering};

pub struct Once(AtomicU8);

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

#[stable(feature = "kernel_core_api", since = "0.1.0")]
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
    pub const fn new() -> Self {
        Self(AtomicU8::new(State::Uncalled.const_into_u8()))
    }

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

    pub fn is_complete(&self) -> bool {
        let state = self.0.load(Ordering::Relaxed).try_into().unwrap();
        match state {
            State::Called => true,
            _ => false
        }
    }
}

pub struct OnceLock<T> {
    data: UnsafeCell<MaybeUninit<T>>,
    once: Once
}

unsafe impl<T: Send> Send for OnceLock<T> {}
unsafe impl<T: Send + Sync> Sync for OnceLock<T> {}

impl<T> OnceLock<T> {
    pub const fn new() -> Self {
        Self {
            data: UnsafeCell::new(MaybeUninit::uninit()),
            once: Once::new()
        }
    }

    pub fn get(&self) -> Option<&T> {
        if !self.once.is_complete() { return None; }
        fence(Ordering::Acquire);

        unsafe {
            Some((*self.data.get()).assume_init_ref())
        }
    }

    pub fn get_mut(&mut self) -> Option<&mut T> {
        if !self.once.is_complete() { return None; }
        fence(Ordering::Acquire);

        unsafe {
            Some((*self.data.get()).assume_init_mut())
        }
    }

    pub fn get_or_init(&self, f: impl FnOnce() -> T) -> &T {
        self.once.call_once(|| unsafe { (*self.data.get()).write(f()); });
        unsafe { (*self.data.get()).assume_init_ref() }
    }
}

pub struct LazyLock<T, F: FnOnce() -> T = fn() -> T> {
    once: OnceLock<T>,
    // FIXME: actually drop this when needed
    f: MaybeUninit<F>
}

unsafe impl<T, F: FnOnce() -> T> Send for LazyLock<T, F> {}
unsafe impl<T, F: FnOnce() -> T> Sync for LazyLock<T, F> {}

impl<T, F: FnOnce() -> T> LazyLock<T, F> {
    pub const fn new(f: F) -> Self {
        Self {
            once: OnceLock::new(),
            f: MaybeUninit::new(f)
        }
    }

    pub fn force(this: &Self) -> &T {
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

pub use bootstrap::BootstrapOnceLock;

mod bootstrap {
    use core::cell::UnsafeCell;
    use core::mem::MaybeUninit;
    use core::sync::atomic::{AtomicU8, fence, Ordering};

    #[derive(Debug, Copy, Clone, Eq, PartialEq)]
    #[repr(u8)]
    enum State {
        Uncalled = 0,
        Saving = 1,
        Running = 2,
        Init = 3,
        Poison = 4
    }

    impl State {
        const fn const_into_u8(self) -> u8 {
            match self {
                State::Uncalled => 0,
                State::Saving => 1,
                State::Running => 2,
                State::Init => 3,
                State::Poison => 4
            }
        }

        const fn const_from_u8(value: u8) -> Result<Self, ()> {
            match value {
                0 => Ok(State::Uncalled),
                1 => Ok(State::Saving),
                2 => Ok(State::Running),
                3 => Ok(State::Init),
                4 => Ok(State::Poison),
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

    pub struct BootstrapOnceLock<T> {
        data: UnsafeCell<MaybeUninit<T>>,
        state: AtomicU8
    }

    unsafe impl<T> Send for BootstrapOnceLock<T> {}
    unsafe impl<T> Sync for BootstrapOnceLock<T> {}

    impl<T> BootstrapOnceLock<T> {
        pub const fn new() -> Self {
            Self {
                data: UnsafeCell::new(MaybeUninit::uninit()),
                state: AtomicU8::new(State::Uncalled.const_into_u8())
            }
        }

        pub fn get(&self) -> Option<&T> {
            let state: State = self.state.load(Ordering::Relaxed).try_into().unwrap();

            if state == State::Poison { panic!("poisoned `BootstrapOnceLock`") }
            if state == State::Uncalled || state == State::Saving { return None; }
            fence(Ordering::Acquire);

            unsafe {
                Some((*self.data.get()).assume_init_ref())
            }
        }

        /*
        - Starts as `Uncalled`
        - Move to `Saving`
        - Store bootstrap value
        - Move to `Running` - value is now legal to access
        - Call function
        - Move to `Saving` - value is now illegal to access
        - Store new value
        - Move to `Init` - value is now illegal to access
         */
        pub fn bootstrap(&self, bootstrap_value: T, f: impl FnOnce() -> T) -> &T {
            loop {
                let current = self.state.compare_exchange_weak(State::Uncalled.into(), State::Saving.into(), Ordering::Relaxed, Ordering::Acquire);
                match current {
                    Ok(_) => break, // Switched from Uncalled to Saving, bootstrap then call the function
                    Err(s) if s == State::Poison.into() => panic!("poisoned `BootstrapOnceLock`"),
                    Err(s) if s == State::Running.into() || s == State::Saving.into() => {}, // Currently running, spin until state changes
                    Err(s) if s == State::Init.into() => {
                        // Already called, return immediately
                        return unsafe {
                            (*self.data.get()).assume_init_ref()
                        };
                    },
                    Err(s) if s == State::Uncalled.into() => {}, // Weak CAS failure so retry
                    _ => unreachable!()
                }
                core::hint::spin_loop();
            }

            // We now need to bootstrap and init
            unsafe { (*self.data.get()).write(bootstrap_value); }
            // Release ordering so bootstrapped value syncs with Acquire ordering in Self::get
            self.state.store(State::Running.into(), Ordering::Release);

            let true_value = f();

            // Relaxed ordering since no memory stuff to sync with (???)
            self.state.store(State::Saving.into(), Ordering::Relaxed);
            let ret = unsafe { (*self.data.get()).write(true_value) };
            // Release ordering so bootstrapped value syncs with Acquire ordering in Self::get
            self.state.store(State::Init.into(), Ordering::Release);
            ret
        }
    }
}
