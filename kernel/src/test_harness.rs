use core::alloc::{GlobalAlloc, Layout};
use core::fmt;
use core::fmt::Write;
use crate::{panicking::do_panic, panicking, sprint, sprintln};
use kernel_api::sync::Mutex;
use core::panic::PanicInfo;
use test::{ShouldPanic, TestDescAndFn, TestFn};

static CURRENT_TEST: Mutex<Option<ShouldPanic>> = Mutex::new(None);

pub enum Result { Success, Fail, Ignored }

#[panic_handler]
fn panic_handler(info: &PanicInfo) -> ! {
	let Some(current_test) = *CURRENT_TEST.lock() else {
		// no test running so just normal panic
		sprintln!("{info}");
		loop {}
	};

	match current_test {
		// If a test fails we want stdout to come after the FAILED line
		// Without a proper heap we have no way to pass the panic info up the stack, so print "FAILED" here along with panic info
		// The test harness then does nothing
		ShouldPanic::No => {
			sprintln!("\u{001b}[31mFAILED\u{001b}[0m");
			sprintln!("---- stdout ----");
			sprintln!("{info}");
			sprintln!("----------------");
		},
		ShouldPanic::Yes => sprintln!("\u{001b}[32mok\u{001b}[0m"),
		ShouldPanic::YesWithMessage(msg) => {
			struct StrFormatCmp(&'static str);
			impl Write for StrFormatCmp {
				fn write_str(&mut self, s: &str) -> fmt::Result {
					if s.len() > self.0.len() { return Err(fmt::Error); }
					if s != &self.0[..s.len()] { return Err(fmt::Error); }

					self.0 = &self.0[s.len()..];
					Ok(())
				}
			}

			let mut cmp = StrFormatCmp(msg);
			let result = match info.message() {
				Some(&args) => cmp.write_fmt(args),
				None => Err(fmt::Error)
			};

			match result {
				Ok(_) => sprintln!("\u{001b}[32mok\u{001b}[0m"),
				Err(_) => {
					sprintln!("\u{001b}[31mFAILED\u{001b}[0m");
					sprintln!("---- stdout ----");
					sprintln!("{info}");
					sprintln!("note: panic did not match expected message");
					match info.message() {
						Some(&args) => sprintln!("\tpanic message: `{args}`"),
						None => sprintln!("\tno panic message")
					};
					sprintln!("\texpected: `{msg}`");
					sprintln!("----------------");
				}
			}
		}
	}

	do_panic()
}

fn run_normal_test(test: &TestFn) -> Result {
	match panicking::catch_unwind(|| test.run()) {
		Ok(_) => {
			// no panic so nothing printed by panic handler
			sprintln!("\u{001b}[32mok\u{001b}[0m");
			Result::Success
		},
		Err(_) => {
			// panic handler prints all failure output so do nothing
			Result::Fail
		}
	}
}

fn run_panic_test(test: &TestFn) -> Result {
	match panicking::catch_unwind(|| test.run()) {
		Ok(_) => {
			// no panic so nothing printed by panic handler
			sprintln!("\u{001b}[31mFAILED\u{001b}[0m");
			sprintln!("---- stdout ----");
			sprintln!("note: test did not panic as expected");
			sprintln!("----------------");
			Result::Fail
		},
		Err(_) => {
			// panic handler prints all success output so do nothing
			Result::Success
		},
	}
}

fn run_test(test: &TestDescAndFn) -> Result {
	let TestDescAndFn { desc, testfn } = test;

	let should_panic_text = if desc.should_panic != ShouldPanic::No { " - should panic" } else { "" };
	sprint!("test {}{} ... ", desc.name, should_panic_text);

	if desc.ignore {
		sprint!("\u{001b}[33mignored");
		if let Some(reason) = desc.ignore_message {
			sprint!(", {reason}");
		}
		sprintln!("\u{001b}[0m");
		return Result::Ignored;
	}

	*CURRENT_TEST.lock() = Some(desc.should_panic);

	let ret = match desc.should_panic {
		ShouldPanic::Yes | ShouldPanic::YesWithMessage(_) => run_panic_test(testfn),
		ShouldPanic::No => run_normal_test(testfn),
	};

	CURRENT_TEST.lock().take();
	ret
}

pub fn test_runner(tests: &[&TestDescAndFn]) -> ! {
	sprintln!("running {} tests", tests.len());

	let mut success_count = 0;
	let mut ignore_count = 0;
	for &test in tests {
		match run_test(test) {
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
	unreachable!("qemu did not exit")
}

#[cfg_attr(test, global_allocator)]
static ALLOCATOR: Foo = Foo(Mutex::new(FooInner {
	buffer: [0; 20],
	used: false,
}));

struct Foo(Mutex<FooInner>);

struct FooInner {
	buffer: [u64; 20],
	used: bool
}

unsafe impl GlobalAlloc for Foo {
	unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
		let mut this = self.0.lock();
		if this.used || layout.size() > (this.buffer.len() * 8) || layout.align() > 8 { core::ptr::null_mut() }
		else {
			this.used = true;
			this.buffer.as_mut_ptr().cast()
		}
	}

	unsafe fn dealloc(&self, _: *mut u8, _: Layout) {
		self.0.lock().used = false;
	}
}
