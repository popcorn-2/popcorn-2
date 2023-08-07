use core::fmt::{self, Write};
use bitflags::{bitflags, Flags};
use kernel_exports::sync::Mutex;
use crate::arch::Port;
use crate::sync::late_init::LateInit;

pub static SERIAL0: Mutex<LateInit<SerialPort>> = Mutex::new(LateInit::new());

pub fn init_serial0() -> Result<(), Error> {
	SERIAL0.lock().unwrap()
			.init(unsafe { SerialPort::new(0x3f8) }? );
	Ok(())
}

bitflags! {
	struct IrqEnableFlags: u8 {
		const READ_AVAILABE = 1<<0;
		const TRANSMIT_ERROR = 1<<1;
		const ERROR = 1<<2;
		const STATUS_CHANGE = 1<<3;
	}

	struct LineStatusFlags: u8 {
		const DATA_READ_READY = 1<<0;
		const OVERRUN_ERROR = 1<<1;
		const PARITY_ERROR = 1<<2;
		const FRAMING_ERROR = 1<<3;
		const BREAK_ERROR = 1<<4;
		const DATA_WRITE_READY = 1<<5;
	}
}

impl From<u8> for IrqEnableFlags {
	fn from(value: u8) -> Self {
		Self::from_bits_retain(value)
	}
}
impl From<IrqEnableFlags> for u8 {
	fn from(val: IrqEnableFlags) -> Self {
		val.bits()
	}
}
impl From<u8> for LineStatusFlags {
	fn from(value: u8) -> Self {
		Self::from_bits_retain(value)
	}
}
impl From<LineStatusFlags> for u8 {
	fn from(val: LineStatusFlags) -> Self {
		val.bits()
	}
}

pub struct SerialPort {
	data: Port<u8>,
	irq_enable: Port<u8>,
	fifo_control: Port<u8>,
	line_control: Port<u8>,
	modem_control: Port<u8>,
	line_status: Port<u8>,
	_modem_status: Port<u8>,
	_scratch: Port<u8>
}

impl SerialPort {
	/// Creates a new `SerialPort` and runs a self-test
	///
	/// # Safety
	///
	/// It is up to the caller to guarantee the address provided points to a valid serial port
	pub unsafe fn new(base_addr: u16) -> Result<Self, Error> {
		let mut s = Self {
			data: Port::new(base_addr),
			irq_enable: Port::new(base_addr + 1),
			fifo_control: Port::new(base_addr + 2),
			line_control: Port::new(base_addr + 3),
			modem_control: Port::new(base_addr + 4),
			line_status: Port::new(base_addr + 5),
			_modem_status: Port::new(base_addr + 6),
			_scratch: Port::new(base_addr + 7),
		};

		s.init();
		s.self_test()?;
		Ok(s)
	}

	fn init(&mut self) {
		unsafe {
			self.irq_enable.write(IrqEnableFlags::empty().into());   // Disable interrupts
			self.line_control.write(0x80);   // Set DLAB bit (maps first two ports to baud divisor)
			self.data.write(3);              // Set divisor to 3 (38400 baud)
			self.irq_enable.write(IrqEnableFlags::empty().into());
			self.line_control.write(0x03);   // 8 bits, no parity, one stop bit
			self.fifo_control.write(0xC7);   // Enable FIFO, clear them, with 14-byte threshold
		}
	}

	/// Perform a test of the serial port by enabling loopback mode and checking received data
	fn self_test(&mut self) -> Result<(), Error> {
		unsafe {
			self.modem_control.write(0x1E);   // Set to loopback mode

			self.data.write(0xAE);   // Write to port
			if self.data.read() != 0xAE { return Err(Error::LoopbackFail); }

			self.modem_control.write(0x0F);   // Turn off loopback, enable IRQs
		}
		Ok(())
	}

	/// Sends a single byte of data
	pub fn send(&mut self, data: u8) {
		self.wait_transmit_empty();
		unsafe { self.data.write(data); }
	}

	/// Blocks until transmit buffer is empty
	pub fn wait_transmit_empty(&self) {
		unsafe {
			while !LineStatusFlags::from(self.line_status.read()).contains(LineStatusFlags::DATA_WRITE_READY) {}
		}
	}
}

impl fmt::Write for SerialPort {
	fn write_str(&mut self, s: &str) -> fmt::Result {
		for data in s.as_bytes() {
			self.send(*data);
		}
		Ok(())
	}
}

#[derive(Debug, Copy, Clone)]
pub enum Error {
	LoopbackFail
}

impl fmt::Display for Error {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			Self::LoopbackFail => f.write_str("Serial port - Loopback test failed")
		}
	}
}

#[doc(hidden)]
pub fn _print(args: fmt::Arguments) {
	SERIAL0.lock().unwrap().write_fmt(args).unwrap();
}

#[macro_export]
macro_rules! sprintln {
    () => { $crate::sprint!("\n") };
	($($arg:tt)*) => { $crate::sprint!("{}\n", format_args!($($arg)*)) }
}

#[macro_export]
macro_rules! sprint {
	($($arg:tt)*) => { $crate::io::serial::_print(format_args!($($arg)*)) }
}
