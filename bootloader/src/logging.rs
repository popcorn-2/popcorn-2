use core::cell::UnsafeCell;
use core::fmt::Write;
use core::mem;
use core::ptr::NonNull;
use log::{Level, LevelFilter, Log, Metadata, Record, SetLoggerError};
use crate::framebuffer::FontStyle;

pub trait FormatWrite: Write {
	fn set_color(&mut self, color: (u8, u8, u8));
	fn set_font_style(&mut self, style: FontStyle);
}

static mut LOGGER: Logger = Logger { ui: None, uart: None };

struct Logger {
	ui: Option<NonNull<dyn FormatWrite>>,
	uart: Option<NonNull<dyn Write>>
}

unsafe impl Send for Logger {}
unsafe impl Sync for Logger {}

pub unsafe fn init(uart: &mut dyn Write) -> Result<(), SetLoggerError> {
	LOGGER.uart = Some(NonNull::from(mem::transmute::<_, &'static _>(uart)));

	log::set_logger(&LOGGER)
		.map(move |_| log::set_max_level(log::STATIC_MAX_LEVEL))
}

pub unsafe fn add_ui(ui: &mut dyn FormatWrite) {
	LOGGER.ui = Some(NonNull::from(mem::transmute::<_, &'static _>(ui)));
}

impl Log for Logger {
	fn enabled(&self, _metadata: &Metadata) -> bool {
		self.uart.is_some()
	}

	fn log(&self, record: &Record) {
		unsafe {
			if let Some(mut uart) = self.uart {
				let mut uart = uart.as_mut();
				writeln!(uart, "{}: {}", record.level(), record.args()).unwrap();
			}

			if let Some(mut ui) = self.ui {
				let ui = ui.as_mut();
				let prefix = match record.level() {
					Level::Error => { ui.set_color(colors::ERROR); "E" },
					Level::Warn => { ui.set_color(colors::WARN); "W" },
					Level::Info => { ui.set_color(colors::INFO); "I" },
					_ => { ui.set_color(colors::DEBUG); "D" }
				};
				write!(ui, "[{}] ", prefix).unwrap();
				ui.set_color(colors::DEBUG);
				writeln!(ui, "{}", record.args()).unwrap();
			}
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
