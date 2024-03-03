use core::fmt::{Debug, Display};
use core::time::Duration;

pub trait Timer {
	fn get() -> Self;
	fn set_irq_number(&mut self, irq: usize) -> Result<(), impl Debug>;
	fn get_time_periods(&self) -> Result<Duration, impl Debug>;
	fn get_divisors(&self) -> impl IntoIterator<Item = u64>;
	fn set_divisor(&mut self, divisor: u64) -> Result<(), impl Debug>;
	fn set_oneshot_time(&mut self, ticks: u128) -> Result<(), impl Debug>;
	fn start_periodic(&mut self, ticks: u128) -> Result<(), impl Debug>;
	fn stop_periodic(&mut self);
	fn eoi(&mut self);
}
