use alloc::string::String;
use core::ffi::CStr;
use core::fmt::Write;
use core::mem;
use core::ptr::NonNull;

use log::{Level, Log, Metadata, Record, SetLoggerError};
use lvgl2::misc::Color;

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

			if let Some(mut ui) = self.ui {
				if record.level() > Level::Debug { return; }

				let ui = ui.as_mut();
				let prefix = match record.level() {
					Level::Error => { ui.set_color(colors::ERROR); "E" },
					Level::Warn => { ui.set_color(colors::WARN); "W" },
					Level::Info => { ui.set_color(colors::INFO); "I" },
					_ => { ui.set_color(colors::DEBUG); "D" }
				};
				write!(ui, "[{prefix}] ").unwrap();
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

pub struct LvglLogger {
	pub label: lvgl2::object::label::Label,
	pub current_color: Color,
	pub buffer: String,
}

impl Write for LvglLogger {
	fn write_str(&mut self, s: &str) -> core::fmt::Result {
		for c in s.chars() {
			if c == '\n' {
				self.buffer.push('\0');
				let c_buf = CStr::from_bytes_until_nul(self.buffer.as_bytes()).unwrap();
				self.label.set_text(c_buf);
				self.buffer.clear();
				let _ = self.buffer.write_fmt(format_args!("#{:02}{:02}{:02}", self.current_color.r(), self.current_color.g(), self.current_color.b()));
			} else { self.buffer.push(c); }
		}

		Ok(())
	}
}

impl FormatWrite for LvglLogger {
	fn set_color(&mut self, color: (u8, u8, u8)) {
		self.current_color = Color::from_rgb(color.0, color.1, color.2);
	}

	fn set_font_style(&mut self, style: FontStyle) {

	}
}
