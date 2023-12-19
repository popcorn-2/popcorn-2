#![no_std]

#![feature(never_type)]

extern crate alloc;

use alloc::format;
use alloc::string::String;
use core::fmt::{Debug, Display, Formatter};

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
	StaticTestFn(fn() -> Result<(), String>),
}

pub use TestFn::*;

impl TestFn {
	pub fn run(&self) -> Result<(), String> {
		match self {
			StaticTestFn(f) => f()
		}
	}
}

#[derive(Copy, Clone)]
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

impl From<ShouldPanic> for bool {
	fn from(value: ShouldPanic) -> Self {
		match value {
			ShouldPanic::No => false,
			_ => true
		}
	}
}

pub trait Termination {
	fn status(&self) -> Result<(), String>;
}

impl Termination for ! {
	fn status(&self) -> Result<(), String> {
		*self
	}
}

impl Termination for () {
	fn status(&self) -> Result<(), String> {
		Ok(())
	}
}

impl<T: Termination, E: Debug> Termination for Result<T, E> {
	fn status(&self) -> Result<(), String> {
		match self {
			Ok(ref ok) => Ok(()),
			Err(ref e) => Err(format!("{e:?}"))
		}
	}
}

pub fn assert_test_result(result: impl Termination) -> Result<(), String> {
	result.status()
}
