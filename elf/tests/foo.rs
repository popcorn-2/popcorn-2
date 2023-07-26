use std::{fs, mem};
use std::fs::File;
use std::io::Read;
use std::ptr::slice_from_raw_parts_mut;

const EXECUTABLE_DATA: &'static [u8] = include_bytes!("kernel.exec");
const SHARED_LIBRARY_DATA: &'static [u8] = include_bytes!("allocator.kmod");

#[test]
fn relocate_and_link() {
	let mut exe_data = vec![0u64; ((EXECUTABLE_DATA.len() + 7) / 8) as usize];
	let exe_data = unsafe {
		let byte_buf = &mut *slice_from_raw_parts_mut(exe_data.as_mut_ptr().cast::<u8>(), exe_data.len() * 8);
		byte_buf.copy_from_slice(EXECUTABLE_DATA);
		byte_buf
	};
	let kernel = match elf::File::try_new(exe_data) {
		Ok(f) => f,
		Err(e) => panic!("{e}")
	};

	let sym = kernel.exported_symbols();

	let mut so_data = vec![0u64; ((SHARED_LIBRARY_DATA.len() + 7) / 8) as usize];
	let so_data = unsafe {
		let byte_buf = &mut *slice_from_raw_parts_mut(so_data.as_mut_ptr().cast::<u8>(), so_data.len() * 8);
		byte_buf.copy_from_slice(SHARED_LIBRARY_DATA);
		byte_buf
	};
	let mut file = match elf::File::try_new(so_data) {
		Ok(f) => f,
		Err(e) => panic!("{e}")
	};

	file.relocate(0xf648329f000);
	file.link(&sym).unwrap();
}