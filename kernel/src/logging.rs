use log::{Record, Level, Metadata, SetLoggerError};

struct SimpleLogger;

impl log::Log for SimpleLogger {
	fn enabled(&self, meta: &Metadata) -> bool {
		if meta.level() > Level::Debug {
			!(false
				|| meta.target().contains("unwinder")
				|| meta.target().contains("paging")
			)
		} else { true }
	}

	fn log(&self, record: &Record) {
		if self.enabled(record.metadata()) {
			match record.level() {
				Level::Error => sprint!("\u{001b}[31m\u{001b}[1mERROR\u{001b}[0m\u{001b}[1m"),
				Level::Warn => sprint!("\u{001b}[33m\u{001b}[1mWARN\u{001b}[0m\u{001b}[1m"),
				Level::Info => sprint!("\u{001b}[35mINFO\u{001b}[0m"),
				Level::Debug => sprint!("\u{001b}[34mDEBUG\u{001b}[0m"),
				Level::Trace => sprint!("\u{001b}[0mTRACE")
			}
			sprint!(": ");

			if let Some(current_thread) = percpu_v2!(current_thread).try_read()
					&& let Some(current_thread) = current_thread.as_ref() {
				sprint!("[{} `{}`] ", current_thread.thread_id.get(), current_thread.name);
			}

			if let Some(file) = record.file() && let Some(line) = record.line() {
				sprint!("{}:{} - ", file, line);
			}

			sprintln!("{}\u{001b}[0m", record.args());
		}
	}

	fn flush(&self) {}
}

static LOGGER: SimpleLogger = SimpleLogger;

pub fn init() -> Result<(), SetLoggerError> {
	log::set_logger(&LOGGER)
			.map(|()| log::set_max_level(log::STATIC_MAX_LEVEL))
}
