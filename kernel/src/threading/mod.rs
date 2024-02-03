use kernel_api::memory::mapping::Stack;

pub struct ThreadControlBlock {
	name: &'static str,
	kernel_stack: Stack<'static>
}

pub enum ThreadState {
	Ready
}
