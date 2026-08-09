use alloc::vec::Vec;
use core::fmt::Debug;
use core::ops::Range;
use core::{fmt, ptr};
use elf::{segment, File, OsAbi, Isa};
use kernel_api::memory::VirtualAddress;

pub struct KernelLoadInfo {
	pub kernel: File<Vec<u8>>,
	//pub page_table: PageTable,
	pub address_range: Range<VirtualAddress>,
	pub kasan_enabled: bool,
	pub stack_page_count: u32,
}

#[derive(Debug)]
pub enum Error {
	ElfError(elf::ParseError),
	UefiError(uefi::Error),
	NoStack,
	NoKasan,
	WrongAbi,
	WrongArchitecture,
	MalformedKernel,
}

impl From<elf::ParseError> for Error {
	fn from(value: elf::ParseError) -> Self {
		Self::ElfError(value)
	}
}

impl From<uefi::Error> for Error {
	fn from(value: uefi::Error) -> Self {
		Self::UefiError(value)
	}
}

impl fmt::Display for Error {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			Error::ElfError(error) => write!(f, "elf error: {error}"),
			Error::UefiError(error) => write!(f, "uefi error: {error}"),
			Error::MalformedKernel | Error::WrongArchitecture | Error::NoKasan | Error::NoStack | Error::WrongAbi => write!(f, "malformed kernel image"),
		}
	}
}

impl core::error::Error for Error {
	fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
		match self {
			Error::ElfError(error) => Some(error),
			Error::UefiError(error) => Some(error),
			Error::MalformedKernel | Error::WrongArchitecture | Error::NoKasan | Error::NoStack | Error::WrongAbi => None,
		}
	}
}

pub fn load_kernel(from: Vec<u8>) -> Result<KernelLoadInfo, Error> {
	let kernel = File::try_new(from)?;

	if kernel.abi().os != OsAbi::SYSTEM_V && kernel.abi().os != OsAbi::POPCORN { return Err(Error::WrongAbi); }
	if kernel.isa() != cfg_select! {
		target_arch = "x86_64" => Isa::X86_64,
	} { return Err(Error::WrongArchitecture); }

	let kernel_first_page = kernel.segments()
		.filter(|segment| segment.ty() == segment::Type::LOAD)
		.map(|segment| segment.vaddr())
		.min()
		.map(|addr| VirtualAddress::new(addr.try_into().expect("kernel image too large for system")))
		.ok_or(Error::MalformedKernel)?;

	let kernel_last_page = kernel.segments()
		.filter(|segment| segment.ty() == segment::Type::LOAD)
		.map(|segment| segment.vaddr())
		.max()
		.map(|addr| VirtualAddress::new(addr.try_into().expect("kernel image too large for system")))
		.ok_or(Error::MalformedKernel)?;

	/*let mut page_table = PageTable::new()?;

	kernel.load_with(|segment, data| {
		assert!(segment.align() <= boot::PAGE_SIZE, "Not designed for >1 page alignment");

		let mut flags = TableEntryFlags::empty();
		if segment.flags().contains(segment::Flags::Writeable) { flags |= TableEntryFlags::WRITABLE; }
		let mem_type = if segment.flags().contains(segment::Flags::Executable) { MemoryType::LOADER_CODE }
			else {
				flags |= TableEntryFlags::NO_EXECUTE;
				MemoryType::LOADER_DATA
			};

		let page_count = usize::try_from(segment.mem_size()).expect("Size of segment cannot fit in `usize`")
			.div_ceil(PAGE_SIZE);

		let allocation = boot::allocate_pages(
			AllocateType::AnyPages,
			mem_type,
			page_count,
		)?;

		unsafe {
			ptr::write_bytes(
				allocation.as_ptr().byte_add(data.len()),
				0,
				page_count * boot::PAGE_SIZE,
			);

			ptr::copy_nonoverlapping(
				data.as_ptr(),
				allocation.as_ptr(),
				data.len(),
			);
		}

		page_table.try_map_range_with(
			Page(segment.vaddr()),
			Frame(allocation),
			page_count.try_into().unwrap(),
			flags,
			crate::paging_reasons::kernel_seg_to_mapping_ty(segment.ty(), segment.flags()),
		).unwrap();

		Ok(())
	})?;*/
	
	let kernel_first_page = kernel_first_page - 8usize*1024*1024; // vmem bootstrap region

	let kasan_enabled = {
		let note = kernel.notes().find(|note| note.name() == c"Popcorn" && note.ty() == 1).ok_or(Error::NoKasan)?;
		note.description()[0] != 0
	};

	let stack_page_count = {
		let note = kernel.notes().find(|note| note.name() == c"Popcorn" && note.ty() == 2).ok_or(Error::NoStack)?;
		let payload = <[u8; 4]>::try_from(note.description()).map_err(|_| Error::NoStack)?;
		u32::from_ne_bytes(payload)
	};

	Ok(KernelLoadInfo {
		kernel,
		//page_table,
		address_range: kernel_first_page..kernel_last_page,
		kasan_enabled,
		stack_page_count,
	})
}
