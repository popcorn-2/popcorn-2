use core::fmt;
use core::fmt::Write as _;
use core::time::Duration;
use log::{Level, LevelFilter, Log, Metadata, Record, SetLoggerError as SetLoggerErrorInner};
use uefi::proto::console::text::{Color, Key};
use uefi::{boot, println, system, Char16};

static LOGGER: Logger = Logger;

struct Logger;

#[derive(Debug)]
pub struct SetLoggerError(SetLoggerErrorInner);

impl From<SetLoggerErrorInner> for SetLoggerError {
	fn from(inner: SetLoggerErrorInner) -> Self {
		Self(inner)
	}
}

impl core::error::Error for SetLoggerError {}

impl fmt::Display for SetLoggerError {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		self.0.fmt(f)
	}
}

/// # Errors
///
/// Returns an error if called more than once.
pub fn init() -> Result<(), SetLoggerError> {
	log::set_logger(&LOGGER)?;
	#[cfg(debug_assertions)] log::set_max_level(LevelFilter::Debug);
	#[cfg(not(debug_assertions))] log::set_max_level(LevelFilter::Info);

	check_verbose_mode();

	Ok(())
}

fn check_verbose_mode() {
	const TIMEOUT: Duration = Duration::from_secs(2);
	const SLEEP: Duration = Duration::from_millis(50);

	// SAFETY: all ASCII chars are valid UCS-2
	const CHAR16_VL: Char16 = unsafe { Char16::from_u16_unchecked(b'v' as u16) };
	// SAFETY: all ASCII chars are valid UCS-2
	const CHAR16_VU: Char16 = unsafe { Char16::from_u16_unchecked(b'V' as u16) };

	println!("Press `V` to enable verbose mode");

	let verbose_mode = system::with_stdin(|stdin| {
		for _ in 0..TIMEOUT.div_duration_ceil(SLEEP) {
			if let Ok(Some(Key::Printable(CHAR16_VL | CHAR16_VU))) = stdin.read_key() {
				return true;
			}
			boot::stall(SLEEP);
		}
		false
	});

	if verbose_mode {
		println!("Verbose mode enabled");
		log::set_max_level(LevelFilter::Debug);
	}
}

impl Log for Logger {
	fn enabled(&self, _metadata: &Metadata<'_>) -> bool {
		true
	}

	fn log(&self, record: &Record<'_>) {
		system::with_stdout(|stdout| {
			if record.target() != "<continuation>" {
				let _ = match record.level() {
					Level::Error => stdout.set_color(Color::Red, Color::Black),
					Level::Warn => stdout.set_color(Color::Yellow, Color::Black),
					Level::Info => stdout.set_color(Color::Magenta, Color::Black),
					Level::Debug => stdout.set_color(Color::Cyan, Color::Black),
					Level::Trace => stdout.set_color(Color::White, Color::Black),
				};

				let _ = write!(stdout, "{}: ", record.level());

				if let Some(file) = record.file() && let Some(line) = record.line() {
					let _ = write!(stdout, "{file}:{line} - ");
				}
			}

			let _ = stdout.set_color(Color::White, Color::Black);
			let _ = writeln!(stdout, "{}", record.args());
		});
	}

	fn flush(&self) {}
}
