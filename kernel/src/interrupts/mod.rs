use crate::hal;
use crate::hal::interrupts_v2::Vector;

#[expect(unreachable_code)]
pub fn global_irq_handler(vector: Vector) {
	if vector == hal::IPI_VECTOR {
		todo!("IPI");
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
