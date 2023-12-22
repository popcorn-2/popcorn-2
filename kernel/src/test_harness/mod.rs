use core::alloc::{GlobalAlloc, Layout};
use core::fmt;
use core::fmt::Write;
use crate::{panicking::do_panic, panicking, sprintln};
use kernel_api::sync::Mutex;
use core::panic::PanicInfo;
use test::{ShouldPanic, TestDescAndFn, TestFn, TestName};
use kernel_hal::Hal;

mod junit;
mod pretty;

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
			<FORMATTER as Formatter>::add_result(false, Some(format_args!("{info}")));
		},
		ShouldPanic::Yes => <FORMATTER as Formatter>::add_result(true, None),
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
				Ok(_) => <FORMATTER as Formatter>::add_result(true, None),
				Err(_) => {
					let none_format = format_args!("None");
					let actual = info.message().unwrap_or(&none_format);

					<FORMATTER as Formatter>::add_result(
						false,
						Some(
							format_args!("{info}\nnote: panic did not match expected message\n\tpanic message: {actual}\n\texpected: `{msg}`")
						)
					);
				}
			}
		}
	}

	do_panic()
}

fn run_normal_test(test: &TestFn) -> Result {
	match panicking::catch_unwind(|| test.run()) {
		Ok(Ok(_)) => {
			// no panic so nothing printed by panic handler
			<FORMATTER as Formatter>::add_result(true, None);
			Result::Success
		},
		Ok(Err(e)) => {
			// no panic so nothing printed by panic handler
			<FORMATTER as Formatter>::add_result(false, Some(format_args!("Error: {e}")));
			drop(e);
			Result::Fail
		},
		Err(_) => {
			// panic handler prints all failure output so do nothing
			Result::Fail
		}
	}
}

fn run_panic_test(test: &TestFn) -> Result {
	match panicking::catch_unwind(|| test.run()) {
		Ok(Ok(_)) => {
			// no panic so nothing printed by panic handler
			<FORMATTER as Formatter>::add_result(false, Some(format_args!("note: test did not panic as expected")));
			Result::Fail
		},
		Ok(Err(_)) => unreachable!("`should_panic` test cannot return fallible type"),
		Err(_) => {
			// panic handler prints all success output so do nothing
			Result::Success
		},
	}
}

fn run_test(test: &TestDescAndFn) -> Result {
	let TestDescAndFn { desc, testfn } = test;

	<FORMATTER as Formatter>::add_test(desc.name, desc.should_panic.into(), desc.ignore, desc.ignore_message);

	if desc.ignore {
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
	<FORMATTER as Formatter>::startup(tests.len());

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

	<FORMATTER as Formatter>::teardown(success, success_count, tests.len() - success_count - ignore_count, ignore_count);

	struct DebugOut;

	impl minicov::CoverageWriter for DebugOut {
		fn write(&mut self, data: &[u8]) -> core::result::Result<(), minicov::CoverageWriteError> {
			kernel_hal::CurrentHal::debug_output(data).map_err(|_| minicov::CoverageWriteError)
		}
	}

    unsafe {
        // Note that this function is not thread-safe! Use a lock if needed.
        minicov::capture_coverage(&mut DebugOut).unwrap();
    }

	if success {
		kernel_hal::CurrentHal::exit(kernel_hal::Result::Success)
	} else {
		kernel_hal::CurrentHal::exit(kernel_hal::Result::Failure)
	}
}

trait Formatter {
	fn startup(num_tests: usize);
	fn teardown(success: bool, success_count: usize, failure_count: usize, ignore_count: usize);

	fn add_test(name: TestName, should_panic: bool, ignored: bool, ignore_message: Option<&'static str>);
	fn add_result(success: bool, stdout: Option<fmt::Arguments<'_>>);
}

#[cfg(feature = "junit_test_out")]
type FORMATTER = junit::JUnit;
#[cfg(not(feature = "junit_test_out"))]
type FORMATTER = pretty::Pretty;

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
