use core::arch::asm;

const PIC1_CMD: u8 = 0x20;
const PIC1_DATA: u8 = 0x21;
const PIC2_CMD: u8 = 0xA0;
const PIC2_DATA: u8 = 0xA1;
const WAIT_PORT: u8 = 0x80;

const MASK: u8 = 0xFF;
const BASE: u8 = 0x20;

pub fn init() {
	unsafe {
		asm!(
			"mov al, 0x11",
			"out {pic1_cmd}, al",
			"in al, {wait}",

			"mov al, 0x11",
			"out {pic2_cmd}, al",
			"in al, {wait}",

			"mov al, {pic1_base}",
			"out {pic1_data}, al",
			"in al, {wait}",

			"mov al, {pic2_base}",
			"out {pic2_data}, al",
			"in al, {wait}",

			"mov al, 1<<2",
			"out {pic1_data}, al",
			"in al, {wait}",

			"mov al, 2",
			"out {pic2_data}, al",
			"in al, {wait}",

			"mov al, 1",
			"out {pic1_data}, al",
			"in al, {wait}",

			"mov al, 1",
			"out {pic2_data}, al",
			"in al, {wait}",

			"mov al, {mask}",
			"out {pic1_data}, al",
			"in al, {wait}",

			"mov al, {mask}",
			"out {pic2_data}, al",
			"in al, {wait}",

			out("al") _,
			pic1_cmd = const PIC1_CMD,
			pic2_cmd = const PIC2_CMD,
			pic1_data = const PIC1_DATA,
			pic2_data = const PIC2_DATA,
			pic1_base = const BASE,
			pic2_base = const BASE + 8,
			mask = const MASK,
			wait = const WAIT_PORT,
		)
	}
}
