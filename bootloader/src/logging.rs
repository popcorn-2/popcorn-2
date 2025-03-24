use alloc::string::String;
use core::ffi::CStr;
use core::fmt::Write;
use core::mem;
use core::ptr::NonNull;

use log::{Level, Log, Metadata, Record, SetLoggerError};

static mut LOGGER: Logger = Logger { uart: None };

struct Logger {
	uart: Option<NonNull<dyn Write>>
}

unsafe impl Send for Logger {}
unsafe impl Sync for Logger {}

pub unsafe fn init(uart: &mut dyn Write) -> Result<(), SetLoggerError> {
	LOGGER =  Logger { uart: Some(NonNull::from(mem::transmute::<_, &'static _>(uart))) };

	log::set_logger(&LOGGER)
		.map(move |_| log::set_max_level(log::STATIC_MAX_LEVEL))
}

impl Log for Logger {
	fn enabled(&self, _metadata: &Metadata) -> bool {
		self.uart.is_some()
	}

	fn log(&self, record: &Record) {
		unsafe {
			if let Some(mut uart) = self.uart {
				let uart = uart.as_mut();

				if self.enabled(record.metadata()) {
					match record.level() {
						Level::Error => write!(uart, "\u{001b}[31m\u{001b}[1mERROR\u{001b}[0m\u{001b}[1m"),
						Level::Warn => write!(uart, "\u{001b}[33m\u{001b}[1mWARN\u{001b}[0m\u{001b}[1m"),
						Level::Info => write!(uart, "\u{001b}[35mINFO\u{001b}[0m"),
						Level::Debug => write!(uart, "\u{001b}[34mDEBUG\u{001b}[0m"),
						Level::Trace => write!(uart, "\u{001b}[0mTRACE")
					};
					write!(uart, ": ");
				}

				if let Some(file) = record.file() && let Some(line) = record.line() {
					let _ = write!(uart, "{}:{} - ", file, line);
				}
				let _ = writeln!(uart, "{}\u{001b}[0m", record.args());
			}
		}
	}

	fn flush(&self) {}
}
