use crate::{panicking::do_panic, panicking, sprint, sprintln};
use kernel_api::sync::Mutex;
use core::panic::PanicInfo;

/// Wrapper struct for tests that succeed if they panic
pub struct ShouldPanic<T>(pub T, pub &'static str);

/// Wrapper struct for tests that should not be run
pub struct Ignored<T>(pub T, pub &'static str);

#[derive(Copy, Clone, Eq, PartialEq)]
pub enum CurrentTestType {
	Normal,
	ShouldPanic
}

static CURRENT_TEST: Mutex<Option<CurrentTestType>> = Mutex::new(None);

pub enum Result { Success, Fail, Ignored }

/// A type that can be run as a test
pub trait Testable {
	fn run(&self) -> Result;
}

impl<T: Fn()> Testable for Ignored<T> {
	fn run(&self) -> Result {
		// Ignored tests just print their name and exit
		sprintln!("test {} ... \u{001b}[33mignored\u{001b}[0m", self.1);
		Result::Ignored
	}
}

impl<T: Fn()> Testable for ShouldPanic<T> {
	fn run(&self) -> Result {
		*CURRENT_TEST.lock() = Some(CurrentTestType::ShouldPanic);

		sprint!("test {} - should panic ... ", self.1);
		let ret = match crate::panicking::catch_unwind(&self.0) {
			// panic handler does nothing so print output here
			Ok(_) => {
				sprintln!("\u{001b}[31mFAILED\u{001b}[0m");
				sprintln!("---- stdout ----");
				sprintln!("note: test did not panic as expected");
				sprintln!("----------------");
				Result::Fail
			},
			Err(_) => {
				sprintln!("\u{001b}[32mok\u{001b}[0m");
				Result::Success
			},
		};

		CURRENT_TEST.lock().take();
		ret
	}
}

impl<T: Fn()> Testable for T {
	fn run(&self) -> Result {
		*CURRENT_TEST.lock() = Some(CurrentTestType::Normal);

		sprint!("test {} ... ", core::any::type_name::<T>());
		let ret = match crate::panicking::catch_unwind(self) {
			Ok(_) => {
				sprintln!("\u{001b}[32mok\u{001b}[0m");
				Result::Success
			},
			Err(_) => {
				// panic handler prints all failure output so do nothing
				Result::Fail
			}
		};

		CURRENT_TEST.lock().take();
		ret
	}
}

#[panic_handler]
fn panic_handler(info: &PanicInfo) -> ! {
	let current_test = CURRENT_TEST.lock().unwrap();

	match current_test {
		// If running a normal test, we want stdout to come after the FAILED line
		// Without a proper heap we have no way to pass the panic info up the stack, so print "FAILED" here along with panic info
		// The test harness then does nothing
		CurrentTestType::Normal => {
			sprintln!("\u{001b}[31mFAILED\u{001b}[0m");
			sprintln!("---- stdout ----");
			sprintln!("{info}");
			sprintln!("----------------");
		},
		_ => {}
	}

	do_panic()
}

pub fn test_runner(tests: &[&dyn Testable]) -> ! {
	sprintln!("running {} tests", tests.len());

	let mut success_count = 0;
	let mut ignore_count = 0;
	for test in tests {
		match test.run() {
			Result::Success => success_count += 1,
			Result::Fail => {},
			Result::Ignored => ignore_count += 1,
		}
	}

	let success = success_count + ignore_count == tests.len();

	sprintln!("\ntest result: {}. {} passed; {} failed; {} ignored",
		if success { "\u{001b}[32mok\u{001b}[0m" } else { "\u{001b}[31mFAILED\u{001b}[0m" },
		success_count,
		tests.len() - success_count - ignore_count,
		ignore_count
	);

	struct QemuDebug;

	impl minicov::CoverageWriter for QemuDebug {
		fn write(&mut self, data: &[u8]) -> core::result::Result<(), minicov::CoverageWriteError> {
			let mut qemu_debug = crate::arch::Port::<u8>::new(0xe9);
			for byte in data {
				unsafe { qemu_debug.write(*byte); }
			}
			Ok(())
		}
	}

    unsafe {
        // Note that this function is not thread-safe! Use a lock if needed.
        minicov::capture_coverage(&mut QemuDebug).unwrap();
    }

	let mut qemu_exit = crate::arch::Port::<u32>::new(0xf4);
	if success {
		unsafe { qemu_exit.write(0x10); }
	} else {
		unsafe { qemu_exit.write(0); }
	}
	unreachable!()
}
