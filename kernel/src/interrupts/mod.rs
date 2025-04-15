#[allow(unused_imports)] use crate::prelude::*;
use core::cell::OnceCell;
use crate::hal;
use crate::hal::interrupts_v2::Vector;

percpu!(static DEFER_IRQ: OnceCell<Box<dyn Fn()>> = OnceCell::new());

pub fn global_irq_handler(vector: Vector) {
	if vector == hal::IPI_VECTOR {
		// todo: check actual IPI cause instead of blindly assuming self-ipi defer
		DEFER_IRQ().get().expect("No defer irq handler")();
		hal::get_and_disable_interrupts();
		hal::send_local_eoi(vector);
	} else if vector == hal::SPURIOUS_VECTOR {
		warn!("Spurious interrupt");
	} else if vector == hal::timing::local_timer().vector() { // todo: can this be nicer?
		crate::timing::local_timer_queue_irq_handler();
		hal::get_and_disable_interrupts();
		hal::send_local_eoi(vector);
	} else {
		warn!("Unhandled IRQ: vector {:#x}", vector.0);
	}
	// todo: extint
}

// todo: no
pub fn set_defer_irq(f: impl Fn() + 'static) {
	DEFER_IRQ().set(Box::new(f)).unwrap_or_else(|_| panic!());
}
