use core::ffi::CStr;
use core::fmt::Debug;
use core::ops::Range;
use core::ptr;

use uefi::fs::Path;
use uefi::table::boot::{AllocateType, PAGE_SIZE};

use elf::File;
use elf::header::program::{ProgramHeaderEntry64, SegmentFlags, SegmentType};
use elf::symbol_table::SymbolMap;
use kernel_api::memory::{PhysicalAddress, VirtualAddress};

use crate::paging::{Frame, Page, PageTable};

pub fn load_module(from: impl AsRef<Path>) -> Result<(),()> {
	todo!()
}

struct LoadedSegment {
	physical_addr: PhysicalAddress,
	virtual_addr: VirtualAddress,
	page_count: usize
}

fn load_segment<E: Debug, F: FnMut(usize, AllocateType) -> Result<u64, E>>(kernel: &File, segment: &ProgramHeaderEntry64, mut allocator: F) -> Result<LoadedSegment, ()> {
	let allocation_type = if segment.segment_flags.contains(SegmentFlags::LowMem) {
		AllocateType::MaxAddress(0x10_0000)
	} else { AllocateType::AnyPages };

	let page_count = usize::try_from(segment.memory_size).expect("Size of segment cannot fit in `usize`")
			.div_ceil(PAGE_SIZE);
	let Ok(allocation) = allocator(page_count, allocation_type) else {
		todo!("Throw an error");
	};

	unsafe {
		ptr::copy_nonoverlapping(
			kernel[segment.file_location()].as_ptr(),
			allocation as *mut _,
			segment.file_size.try_into().unwrap()
		);

		ptr::write_bytes(
			(allocation + segment.file_size) as *mut u8,
			0,
			(segment.memory_size - segment.file_size).try_into().expect("Size of segment cannot fit in `usize`")
		);
	}

	Ok(LoadedSegment {
		physical_addr: PhysicalAddress::new(allocation.try_into().expect("Todo")),
		virtual_addr: VirtualAddress::new(segment.vaddr.try_into().expect("Virtual address could not fit in machine width???")),
		page_count
	})
}

pub struct KernelLoadInfo<'a> {
	pub kernel: File<'a>,
	pub page_table: PageTable,
	pub address_range: Range<VirtualAddress>,
	pub tls: Range<VirtualAddress>
}

pub fn load_kernel<E: Debug, F: FnMut(usize, AllocateType) -> Result<u64, E>>(from: &mut [u8], mut allocator: F) -> Result<KernelLoadInfo<'_>, ()> {
	let kernel = File::try_new(from).map_err(|_| ())?;
	let mut page_table = unsafe { PageTable::try_new(|| allocator(1, AllocateType::AnyPages)) }.map_err(|_| ())?;

	let mut kernel_last_page = VirtualAddress::new(usize::MIN);
	let mut kernel_first_page = VirtualAddress::new(usize::MAX);
	let mut tls_start = Option::<VirtualAddress>::None;
	let mut tls_end = Option::<VirtualAddress>::None;

	kernel.segments().filter(|segment| segment.segment_type == SegmentType::LOAD || segment.segment_type == SegmentType::TLS)
	      .try_for_each(|segment_meta| {
		     let segment = load_segment(&kernel, &segment_meta, &mut allocator)?;

		      if segment.virtual_addr < kernel_first_page { kernel_first_page = segment.virtual_addr }
		      let last_page = segment.virtual_addr + segment.page_count * PAGE_SIZE;
		      if last_page > kernel_last_page { kernel_last_page = last_page };

		      if segment_meta.segment_type == SegmentType::TLS {
			      tls_start = Some(segment.virtual_addr);
			      tls_end = Some(segment.virtual_addr + usize::try_from(segment_meta.memory_size).unwrap());
		      }

		      page_table.try_map_range(
			      Page(segment.virtual_addr.addr.try_into().unwrap()),
			      Frame(segment.physical_addr.addr.try_into().unwrap()),
			      segment.page_count.try_into().unwrap(),
			      || allocator(1, AllocateType::AnyPages)
		      ).unwrap();

		      Ok(())
	      })?;

	Ok(KernelLoadInfo {
		kernel,
		page_table,
		address_range: kernel_first_page..kernel_last_page,
		tls: tls_start.map(|start| start..tls_end.unwrap()).unwrap_or(VirtualAddress::new(0)..VirtualAddress::new(0))
	})
}

trait MetadataSymbols {
	fn get_metadata(&self, key: &CStr) -> Option<&str>;
}

impl MetadataSymbols for SymbolMap<'_> {
	fn get_metadata(&self, key: &CStr) -> Option<&str> {
		self.get(key).and_then(|symbol| {
			/*let author_data = module.data_at_address(symbol.value).unwrap();
			let author_data = unsafe { &*slice_from_raw_parts(author_data, symbol.size.try_into().unwrap()) };
			core::str::from_utf8(author_data).ok()*/
			todo!()
		})
	}
}
