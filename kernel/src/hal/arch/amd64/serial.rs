#[allow(unused_imports)] use crate::prelude::*;
use core::fmt::{self, Arguments, Write};
use bitflags::bitflags;
use kernel_api::sync::{LazyLock, Mutex};
use crate::hal::arch::amd64::port::Port;

static SERIAL0: LazyLock<Mutex<SerialPort>> = LazyLock::new(|| {
	Mutex::new(unsafe { SerialPort::new(0x3f8) }.expect("Unable to start serial port") )
});

bitflags! {
	struct IrqEnableFlags: u8 {
		const READ_AVAILABE = 1<<0;
		const TRANSMIT_ERROR = 1<<1;
		const ERROR = 1<<2;
		const STATUS_CHANGE = 1<<3;
	}

	struct LineStatusFlags: u8 {
		const INPUT_BUFFER_FULL = 1<<0;
		const OVERRUN_ERROR = 1<<1;
		const PARITY_ERROR = 1<<2;
		const FRAMING_ERROR = 1<<3;
		const BREAK_ERROR = 1<<4;
		const OUTPUT_BUFFER_EMPTY = 1<<5;
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

struct SerialPort {
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
			self.modem_control.write(0x0b);
		}
	}

	/// Perform a test of the serial port by enabling loopback mode and checking received data
	fn self_test(&mut self) -> Result<(), Error> {
		unsafe {
			self.modem_control.write(0x13);   // Set to loopback mode

			self.send(0xAE);   // Write to port
			if self.receive() != 0xAE { return Err(Error::LoopbackFail); }

			self.modem_control.write(0x0b);   // Turn off loopback, enable IRQs
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
			while !LineStatusFlags::from(self.line_status.read()).contains(LineStatusFlags::OUTPUT_BUFFER_EMPTY) {
				core::hint::spin_loop();
			}
		}
	}

	pub fn receive(&mut self) -> u8 {
		self.wait_receive_full();
		unsafe { self.data.read() }
	}

	/// Blocks until receive buffer is full
	pub fn wait_receive_full(&self) {
		unsafe {
			while !LineStatusFlags::from(self.line_status.read()).contains(LineStatusFlags::INPUT_BUFFER_FULL) {
				core::hint::spin_loop();
			}
		}
	}
}

impl Write for SerialPort {
	fn write_str(&mut self, s: &str) -> fmt::Result {
		for data in s.as_bytes() {
			self.send(*data);
		}
		Ok(())
	}
}

#[derive(Debug, Copy, Clone)]
enum Error {
	LoopbackFail
}

impl fmt::Display for Error {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			Self::LoopbackFail => f.write_str("Serial port - Loopback test failed")
		}
	}
}

pub struct HalWriter;

impl crate::hal::FormatWriter for HalWriter {
	fn print(args: Arguments) {
		SERIAL0.lock().write_fmt(args).unwrap();
	}
}

#[export_name = "__popcorn_force_unsafe_serial"]
unsafe fn force_serial(s: &str) {
	let mut guard = unsafe { SERIAL0.make_guard_unchecked() };
	let _ = guard.write_str(s);
	core::mem::forget(guard);
}
