use core::fmt::Arguments;
use test::TestName;
use crate::{sprint, sprintln};
use super::Formatter;

pub struct JUnit;

impl Formatter for JUnit {
	fn startup(_: usize) {
		sprintln!(r#"<?xml version="1.0" encoding="UTF-8"?>"#);
		sprintln!(r#"<testsuites name="popcorn2 tests">"#);
		sprintln!(r#"<testsuite name="kernel internals">"#);
	}

	fn teardown(success: bool, success_count: usize, failure_count: usize, ignore_count: usize) {
		sprintln!(r#"</testsuite></testsuites>"#);
	}

	fn add_test(name: TestName, should_panic: bool, ignored: bool, ignore_message: Option<&'static str>) {
		sprintln!(r#"<testcase name="{name}">"#);
		if ignored {
			sprint!(r#"<skipped "#);
			if let Some(msg) = ignore_message {
				sprint!(r#"message="{msg}""#);
			}
			sprintln!(r#"/></testcase>"#)
		}
	}

	fn add_result(success: bool, stdout: Option<Arguments<'_>>) {
		if !success {
			sprint!(r#"<failure>"#);
			if let Some(msg) = stdout {
				sprint!("<system-out>{msg}</system-out>");
			}
			sprintln!(r#"</failure>"#)
		}
		sprintln!(r#"</testcase>"#);
	}
}