use log::{Level, LevelFilter, Log, Metadata, Record, SetLoggerError};

pub struct Logger;

static LOGGER: Logger = Logger;

impl Logger {
	pub fn init() -> Result<(), SetLoggerError> {
		log::set_logger(&LOGGER)
				.map(|_| log::set_max_level(LevelFilter::Warn))
	}
}

impl Log for Logger {
	fn enabled(&self, metadata: &Metadata) -> bool {
		metadata.level() <= Level::Warn
	}

	fn log(&self, record: &Record) {
		if self.enabled(record.metadata()) {
			todo!()
		}
	}

	fn flush(&self) {}
}
