use std::{fs, mem};
use std::fs::File;
use std::io::Read;
use std::ptr::slice_from_raw_parts_mut;

#[test]
fn load_executable() {
	let mut f = File::open("/Users/Eliyahu/hug2/elf/tests/kernel.exec").unwrap();
	let mut data2 = vec![0u64; ((f.metadata().unwrap().len() + 7) / 8) as usize];
	let baz = unsafe {
		&mut *slice_from_raw_parts_mut(data2.as_mut_ptr().cast::<u8>(), data2.len() * 8)
	};
	f.read(baz).unwrap();
	let file = match elf::File::try_new(baz) {
		Ok(f) => f,
		Err(e) => panic!("{e}")
	};
}

#[test]
fn link() {
	let mut f = File::open("/Users/Eliyahu/hug2/elf/tests/kernel.exec").unwrap();
	let mut data2 = vec![0u64; ((f.metadata().unwrap().len() + 7) / 8) as usize];
	let baz = unsafe {
		&mut *slice_from_raw_parts_mut(data2.as_mut_ptr().cast::<u8>(), data2.len() * 8)
	};
	f.read(baz).unwrap();
	let kernel = match elf::File::try_new(baz) {
		Ok(f) => f,
		Err(e) => panic!("{e}")
	};

	let mut f = File::open("/Users/Eliyahu/hug2/esp/efi/popcorn/allocator.kmod").unwrap();
	let mut data2 = vec![0u64; ((f.metadata().unwrap().len() + 7) / 8) as usize];
	let baz = unsafe {
		&mut *slice_from_raw_parts_mut(data2.as_mut_ptr().cast::<u8>(), data2.len() * 8)
	};
	f.read(baz).unwrap();
	let mut file = match elf::File::try_new(baz) {
		Ok(f) => f,
		Err(e) => panic!("{e}")
	};

	file.link(&kernel.exported_symbols()).unwrap();
}

#[test]
fn relocate_and_link() {
	let mut f = File::open("/Users/Eliyahu/hug2/esp/efi/popcorn/kernel.exec").unwrap();
	let mut data2 = vec![0u64; ((f.metadata().unwrap().len() + 7) / 8) as usize];
	let baz = unsafe {
		&mut *slice_from_raw_parts_mut(data2.as_mut_ptr().cast::<u8>(), data2.len() * 8)
	};
	f.read(baz).unwrap();
	let kernel = match elf::File::try_new(baz) {
		Ok(f) => f,
		Err(e) => panic!("{e}")
	};

	let sym = kernel.exported_symbols();

	let mut f = File::open("/Users/Eliyahu/hug2/esp/efi/popcorn/allocator.kmod").unwrap();
	let mut data2 = vec![0u64; ((f.metadata().unwrap().len() + 7) / 8) as usize];
	let baz = unsafe {
		&mut *slice_from_raw_parts_mut(data2.as_mut_ptr().cast::<u8>(), data2.len() * 8)
	};
	f.read(baz).unwrap();
	let mut file = match elf::File::try_new(baz) {
		Ok(f) => f,
		Err(e) => panic!("{e}")
	};

	file.relocate(0xf648329f000);
	file.link(&sym).unwrap();
}