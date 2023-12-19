use core::fmt::Arguments;
use test::{ShouldPanic, TestName};
use crate::{sprint, sprintln};
use super::Formatter;

pub struct Pretty;

impl Formatter for Pretty {
	fn startup(num_tests: usize) {
		sprintln!("running {num_tests} tests");
	}

	fn teardown(success: bool, success_count: usize, failure_count: usize, ignore_count: usize) {
		sprintln!("\ntest result: {}. {} passed; {} failed; {} ignored",
			if success { "\u{001b}[32mok\u{001b}[0m" } else { "\u{001b}[31mFAILED\u{001b}[0m" },
			success_count,
			failure_count,
			ignore_count
		);
	}

	fn add_test(name: TestName, should_panic: bool, ignored: bool, ignore_message: Option<&'static str>) {
		let should_panic_text = if should_panic { " - should panic" } else { "" };
		sprint!("test {}{} ... ", name, should_panic_text);

		if ignored {
			sprint!("\u{001b}[33mignored");
			if let Some(reason) = ignore_message {
				sprint!(", {reason}");
			}
			sprintln!("\u{001b}[0m");
		}
	}

	fn add_result(success: bool, stdout: Option<Arguments<'_>>) {
		if success { sprintln!("\u{001b}[32mok\u{001b}[0m"); }
		else { sprintln!("\u{001b}[31mFAILED\u{001b}[0m"); }

		if let Some(stdout) = stdout {
			sprintln!("---- stdout ----");
			sprintln!("{stdout}");
			sprintln!();
		}
	}
}
