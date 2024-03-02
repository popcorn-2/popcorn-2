use core::fmt::{Debug, Display};
use core::time::Duration;

pub trait Timer {
	fn get() -> Self;
	fn set_irq_number(&mut self, irq: usize) -> Result<(), impl Debug + Display>;
	fn get_time_periods(&self) -> impl IntoIterator<Item = Duration>;
	fn set_time_period(&mut self, period: Duration) -> Result<(), impl Debug + Display>;
	fn set_oneshot_time(&mut self) -> Result<(), impl Debug + Display>;
}
