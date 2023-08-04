use core::arch::asm;
use core::fmt::Formatter;

mod mutex;

#[cfg(feature = "smp")]
mod rwlock;

pub use mutex::Mutex;
#[cfg(feature = "smp")]
pub use rwlock::RwLock;
#[cfg(not(feature = "smp"))]
pub type RwLock<T> = Mutex<T>;

pub type LockResult<Guard> = Result<Guard, PoisonError<Guard>>;
pub type TryLockResult<Guard> = Result<Guard, TryLockError<Guard>>;

pub enum TryLockError<T> {
	Poisoned(PoisonError<T>),
	WouldSpin
}

pub struct PoisonError<T> {
	guard: T
}

impl<T> PoisonError<T> {
	pub fn new(guard: T) -> Self {
		Self {
			guard
		}
	}

	pub fn into_inner(self) -> T { self.guard }
	pub fn get_ref(&self) -> &T { &self.guard }
	pub fn get_mut(&mut self) -> &mut T { &mut self.guard }
}

impl<T> core::fmt::Debug for PoisonError<T> {
	fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
		f.debug_struct("PoisonError").finish_non_exhaustive()
	}
}

impl<T> core::fmt::Display for PoisonError<T> {
	fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
		"poisoned lock: another task failed inside".fmt(f)
	}
}

impl<T> core::error::Error for PoisonError<T> {}

#[derive(Clone)]
#[repr(transparent)]
pub struct Flags(u64);

fn disable_interrupts() -> Flags {
	let flags: u64;
	unsafe {
		asm!("
			pushf
			pop {}
			cli
		", out(reg) flags, options(preserves_flags, nomem))
	}
	Flags(flags)
}

fn reset_interrupts(flags: Flags) {
	// todo: multiarch
	if (flags.0 & 0x0200) != 0 {
		unsafe {
			asm!("sti", options(preserves_flags, nomem));
		}
	}
}
