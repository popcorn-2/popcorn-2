use crate::amd64::port::Port;

pub fn debug_con_write(data: &[u8]) {
	let mut qemu_debug = Port::<u8>::new(0xe9);
	for byte in data {
		unsafe { qemu_debug.write(*byte); }
	}
}

pub fn debug_exit(status: crate::Result) -> ! {
	let mut qemu_exit = Port::<u32>::new(0xf4);

	match status {
		crate::Result::Success => unsafe { qemu_exit.write(0x10); }
		crate::Result::Failure => unsafe { qemu_exit.write(0); }
	}

	unreachable!("qemu did not exit properly")
}
