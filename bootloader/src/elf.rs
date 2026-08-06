use core::fmt::Debug;
use core::ops::Range;
use core::ptr;
use more_asserts::assert_le;
use uefi::table::boot::{AllocateType, PAGE_SIZE};

use elf::{segment, File};
use kernel_api::memory::VirtualAddress;

use crate::paging::{Frame, Page, PageTable, TableEntryFlags};

pub struct KernelLoadInfo<'a> {
	pub kernel: File<&'a mut [u8]>,
	pub page_table: PageTable,
	pub address_range: Range<VirtualAddress>,
}

#[derive(Debug)]
pub enum Error {
	ElfError(elf::ParseError),
	AllocError,
}

impl From<elf::ParseError> for Error {
	fn from(value: elf::ParseError) -> Self {
		Self::ElfError(value)
	}
}

pub fn load_kernel<E: Debug, F: FnMut(usize, AllocateType) -> Result<u64, E>>(from: &mut [u8], mut allocator: F) -> Result<KernelLoadInfo<'_>, Error> {
	let kernel = File::try_new(from)?;
	let mut page_table = unsafe { PageTable::try_new(|count| allocator(count, AllocateType::AnyPages)) }.map_err(|_| Error::AllocError)?;

	let kernel_first_page = kernel.segments()
		.filter(|segment| segment.ty() == segment::Type::LOAD)
		.map(|segment| segment.vaddr())
		.min()
		.map(|addr| VirtualAddress::new(addr.try_into().expect("kernel image too large for system")))
		.expect("kernel must have at least one segment to load");

	let kernel_last_page = kernel.segments()
		.filter(|segment| segment.ty() == segment::Type::LOAD)
		.map(|segment| segment.vaddr())
		.max()
		.map(|addr| VirtualAddress::new(addr.try_into().expect("kernel image too large for system")))
		.expect("kernel must have at least one segment to load");

	kernel.load_with(|segment, data| {
		assert_le!(segment.align(), 4096, "Not designed for >1 page alignment");

		let page_count = usize::try_from(segment.mem_size()).expect("Size of segment cannot fit in `usize`")
			.div_ceil(PAGE_SIZE);
		let Ok(allocation) = allocator(page_count, AllocateType::AnyPages) else {
			return Err(Error::AllocError);
		};

		unsafe {
			ptr::copy_nonoverlapping(
				data.as_ptr(),
				allocation as *mut _,
				data.len(),
			);

			ptr::write_bytes(
				(allocation as usize + data.len()) as *mut u8,
				0,
				usize::try_from(segment.mem_size()).expect("Size of segment cannot fit in `usize`") - data.len(),
			);
		}

		let mut flags = TableEntryFlags::empty();
		if segment.flags().contains(segment::Flags::Writeable) { flags |= TableEntryFlags::WRITABLE; }
		if !segment.flags().contains(segment::Flags::Executable) { flags |= TableEntryFlags::NO_EXECUTE; }

		page_table.try_map_range_with(
			Page(segment.vaddr()),
			Frame(allocation),
			page_count.try_into().unwrap(),
			|| allocator(1, AllocateType::AnyPages),
			flags,
			crate::paging_reasons::kernel_seg_to_mapping_ty(segment.ty(), segment.flags()),
		).unwrap();

		Ok(())
	})?;
	
	let kernel_first_page = kernel_first_page - 8usize*1024*1024; // vmem bootstrap region

	Ok(KernelLoadInfo {
		kernel,
		page_table,
		address_range: kernel_first_page..kernel_last_page,
	})
}
