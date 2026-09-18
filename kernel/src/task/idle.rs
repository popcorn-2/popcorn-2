pub fn idle_entry() {
	crate::ebr::gc_collect();
	crate::hal::wait_for_interrupt();
}
