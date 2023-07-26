use core::cell::UnsafeCell;
use core::fmt::Write;
use core::mem;
use core::ptr::NonNull;
use log::{Level, LevelFilter, Log, Metadata, Record, SetLoggerError};

pub trait FormatWrite: Write {
	fn set_color(&mut self, color: (u8, u8, u8));
}

static mut LOGGER: Logger = Logger { ui: None, uart: None };

struct Logger {
	ui: Option<NonNull<dyn FormatWrite>>,
	uart: Option<NonNull<dyn Write>>
}

unsafe impl Send for Logger {}
unsafe impl Sync for Logger {}

pub unsafe fn init(ui: &mut dyn FormatWrite, uart: &mut dyn Write) -> Result<(), SetLoggerError> {
	LOGGER.ui = Some(NonNull::from(mem::transmute::<_, &'static _>(ui)));
	LOGGER.uart = Some(NonNull::from(mem::transmute::<_, &'static _>(uart)));

	log::set_logger(&LOGGER)
		.map(move |_| log::set_max_level(log::STATIC_MAX_LEVEL))
}

impl Log for Logger {
	fn enabled(&self, _metadata: &Metadata) -> bool {
		self.ui.is_some()
	}

	fn log(&self, record: &Record) {
		unsafe {
			writeln!(&mut *self.uart.unwrap().as_ptr(), "{}: {}", record.level(), record.args()).unwrap();

			let format_output = &mut *self.ui.unwrap().as_ptr();
			let prefix = match record.level() {
				Level::Error => { format_output.set_color(colors::ERROR); "E" },
				Level::Warn => { format_output.set_color(colors::WARN); "W" },
				Level::Info => { format_output.set_color(colors::INFO); "I" },
				_ => { format_output.set_color(colors::DEBUG); "D" }
			};
			write!(format_output, "[{}] ", prefix).unwrap();
			format_output.set_color(colors::DEBUG);
			writeln!(format_output, "{}", record.args()).unwrap();
		}
	}

	fn flush(&self) {}
}

mod colors {
	type Color = (u8, u8, u8);
	pub const ERROR: Color = (0xf7, 0x31, 0x2a);
	pub const WARN: Color = (0xf5, 0xcc, 0x14);
	pub const INFO: Color = (0xa8, 0x14, 0xe3);
	pub const DEBUG: Color = (0xee, 0xee, 0xee);
}
