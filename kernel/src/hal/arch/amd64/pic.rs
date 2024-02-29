#![allow(const_item_mutation)]

use crate::hal::arch::amd64::port::Port;

const MPIC_COMMAND: Port<u8> = Port::<u8>::new(0x20);
const MPIC_DATA: Port<u8> = Port::<u8>::new(0x21);
const SPIC_COMMAND: Port<u8> = Port::<u8>::new(0xa0);
const SPIC_DATA: Port<u8> = Port::<u8>::new(0xa1);

fn remap(master_base: u8, slave_base: u8) {
	assert_eq!(master_base % 8, 0, "Master base vector must be a multiple of 8");
	assert_eq!(slave_base % 8, 0, "Slave base vector must be a multiple of 8");

	let master_mask = unsafe { MPIC_DATA.read() };
	let slave_mask = unsafe { SPIC_DATA.read() };

	let mut wait_port: Port<u8> = Port::new(0x80);
	let mut wait = || unsafe { wait_port.write(0); };

	unsafe {
		MPIC_COMMAND.write(0x11);
		wait();
		SPIC_COMMAND.write(0x11);
		wait();

		MPIC_DATA.write(master_base);
		wait();
		SPIC_DATA.write(slave_base);
		wait();

		MPIC_DATA.write(4);
		wait();
		SPIC_DATA.write(2);
		wait();

		MPIC_DATA.write(1);
		wait();
		SPIC_DATA.write(1);
		wait();

		MPIC_DATA.write(master_mask);
		SPIC_DATA.write(slave_mask);
	}
}

fn set_mask(master: u8, slave: u8) {
	unsafe {
		MPIC_DATA.write(master);
		SPIC_DATA.write(slave);
	}
}

pub fn init() {
	remap(0x20, 0x28);
	set_mask(0xff, 0xff);
}
