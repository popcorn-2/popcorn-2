#![no_std]

use core::fmt::{Display, Formatter};

pub struct TestDescAndFn {
	pub desc: TestDesc,
	pub testfn: TestFn,
}

pub struct TestDesc {
	pub name: TestName,
	pub ignore: bool,
	pub ignore_message: Option<&'static str>,
	pub source_file: &'static str,
	pub start_line: usize,
	pub start_col: usize,
	pub end_line: usize,
	pub end_col: usize,
	pub should_panic: ShouldPanic,
	pub compile_fail: bool,
	pub no_run: bool,
	pub test_type: TestType,
}

pub enum TestFn {
	StaticTestFn(fn()),
}

pub use TestFn::*;

impl TestFn {
	pub fn run(&self) {
		match self {
			StaticTestFn(f) => f()
		}
	}
}

pub enum TestName {
	StaticTestName(&'static str),
}

pub use TestName::*;

impl Display for TestName {
	fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
		match self {
			StaticTestName(s) => f.write_str(s)
		}
	}
}

pub enum TestType {
	UnitTest,
	IntegrationTest,
	DocTest,
	Unknown,
}

#[derive(Eq, PartialEq, Copy, Clone)]
pub enum ShouldPanic {
	No,
	Yes,
	YesWithMessage(&'static str),
}

pub fn assert_test_result(_: ()) {}
