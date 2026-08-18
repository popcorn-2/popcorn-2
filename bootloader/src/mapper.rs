use alloc::boxed::Box;
use alloc::vec;
use alloc::vec::Vec;
use core::fmt::Debug;
use core::{fmt, ptr};
use log::{debug, warn};
use uefi::{boot, Status, StatusExt};
use uefi::boot::{AllocateType, MemoryType};
use elf::{segment, File, OsAbi, Isa};
use elf::segment::Flags;
use kernel_api::mapping::Ty;
use kernel_api::memory::{RawFrame, RawPage, VirtualAddress};
use kernel_api::memory::asan::{count_to_shadow, mem_to_shadow};
use utils::handoff;
use crate::arch::{AlreadyMappedError, PageTable, TableEntryFlags};
use crate::fs::KernelFiles;

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

pub struct Mapper {
	next_page: RawPage,
	page_table: PageTable,
	entrypoint: VirtualAddress,
	stack: handoff::Stack,
	kasan_enabled: bool,
	used_frames: Vec<RawFrame>,
}

impl Mapper {
	pub fn try_new(kernel: KernelFiles) -> Result<Self, Box<dyn core::error::Error>> {
		let kernel = File::try_new(kernel.kernel)?;

		if kernel.abi().os != OsAbi::SYSTEM_V && kernel.abi().os != OsAbi::POPCORN { return Err(Error::WrongAbi.into()); }
		if kernel.isa() != Isa::TARGET { return Err(Error::WrongArchitecture.into()); }

		let kernel_first_page = kernel.segments()
			.filter(|segment| segment.ty() == segment::Type::LOAD)
			.map(|segment| segment.vaddr())
			.min()
			.ok_or(Error::MalformedKernel)?;

		let kernel_first_page = (kernel_first_page - 8usize*1024*1024).align_down_to_page(); // vmem bootstrap region

		let kasan_enabled = {
			let note = kernel.notes().find(|note| note.name() == c"Popcorn" && note.ty() == 1).ok_or(Error::NoKasan)?;
			note.description()[0] != 0
		};

		let stack_page_count = {
			let note = kernel.notes().find(|note| note.name() == c"Popcorn" && note.ty() == 2).ok_or(Error::NoStack)?;
			let payload = <[u8; 4]>::try_from(note.description()).map_err(|_| Error::NoStack)?;
			u32::from_ne_bytes(payload)
		};

		debug!("loading kernel with KASAN: {kasan_enabled}");
		debug!("loading kernel with {stack_page_count} stack pages");

		let mut mapper = Self {
			next_page: kernel_first_page,
			page_table:  PageTable::new()?,
			entrypoint: VirtualAddress::new(kernel.entry_point()),
			stack: handoff::Stack {
				page_count: 0,
				bottom_virt: RawPage::new(0),
				bottom_phys: RawFrame::new(0),
			},
			kasan_enabled,
			used_frames: vec![],
		};

		kernel.load_with(|segment, data| -> Result<(), Box<dyn core::error::Error>> {
			if segment.align() > boot::PAGE_SIZE { Status::UNSUPPORTED.to_result()?; }
			if segment.vaddr().addr != segment.paddr().addr { Status::UNSUPPORTED.to_result()?; }

			let mem = boot::allocate_pages(
				AllocateType::AnyPages,
				MemoryType::LOADER_DATA,
				segment.mem_size().div_ceil(boot::PAGE_SIZE),
			)?;
			let base_frame = RawFrame::new(mem.expose_provenance().get());
			mapper.used_frames.extend(base_frame..(base_frame + segment.mem_size().div_ceil(kernel_api::memory::PAGE_SIZE)));

			let mut flags = TableEntryFlags::GLOBAL;
			if !segment.flags().contains(Flags::Executable) { flags |= TableEntryFlags::NO_EXECUTE; }
			if segment.flags().contains(Flags::Writeable) { flags |= TableEntryFlags::WRITABLE; }

			mapper.page_table.try_map_range_with(
				segment.vaddr().align_down_to_page(),
				base_frame,
				segment.mem_size().div_ceil(kernel_api::memory::PAGE_SIZE),
				flags,
				crate::paging_reasons::kernel_seg_to_mapping_ty(segment.ty(), segment.flags()),
			)?;

			unsafe {
				ptr::write_bytes(
					mem.as_ptr(),
					0,
					segment.mem_size(),
				);

				ptr::copy_nonoverlapping(
					data.as_ptr(),
					mem.as_ptr(),
					data.len(),
				)
			}

			mapper.init_shadow(
				segment.vaddr(),
				segment.mem_size(),
				0x00,
			)?;

			Ok(())
		})?;

		let (stack_phys, stack_virt) = mapper.new_mapping(
			None,
			None,
			stack_page_count as usize,
			Ty::KERNEL_STACK,
		)?;

		mapper.stack = handoff::Stack {
			page_count: stack_page_count as usize,
			bottom_virt: stack_virt,
			bottom_phys: stack_phys
		};

		// add stack guard page
		mapper.next_page = mapper.next_page - 1;
		mapper.init_shadow(
			*mapper.next_page,
			kernel_api::memory::PAGE_SIZE,
			0xf4,
		)?;

		Ok(mapper)
	}

	pub fn new_mapping(
		&mut self,
		physical_base: Option<RawFrame>,
		virtual_base: Option<RawPage>,
		count: usize,
		ty: Ty,
	) -> Result<(RawFrame, RawPage), Box<dyn core::error::Error>> {
		if boot::PAGE_SIZE != kernel_api::memory::PAGE_SIZE { unimplemented!("fixme"); }

		let physical_base = physical_base.map(uefi::Result::Ok).unwrap_or_else(|| {
			Ok(RawFrame::new(
				boot::allocate_pages(
					AllocateType::AnyPages,
					MemoryType::LOADER_DATA,
					count,
				)?.addr().get()
			))
		})?;

		let virtual_base = virtual_base.unwrap_or_else(|| {
			self.next_page = self.next_page - count;
			self.next_page
		});

		let flags = match ty {
			Ty::KERNEL_STACK | Ty::KERNEL_DATA | Ty::MEM_MAP => TableEntryFlags::GLOBAL | TableEntryFlags::WRITABLE | TableEntryFlags::NO_EXECUTE,
			Ty::KERNEL_CODE => TableEntryFlags::GLOBAL,
			Ty::FB => TableEntryFlags::GLOBAL | TableEntryFlags::WRITABLE | TableEntryFlags::NO_EXECUTE | TableEntryFlags::MMIO,
			Ty::LOADER_CODE => TableEntryFlags::empty(),
			Ty::LOADER_DATA => TableEntryFlags::NO_EXECUTE,
			_ => unimplemented!(),
		};

		self.used_frames.extend(physical_base..(physical_base + count));

		self.page_table.try_map_range_with(
			virtual_base,
			physical_base,
			count,
			flags,
			ty,
		)?;

		self.init_shadow(
			*virtual_base,
			count * kernel_api::memory::PAGE_SIZE,
			0x00,
		)?;

		Ok((physical_base, virtual_base))
	}

	fn init_shadow(&mut self, base: VirtualAddress, bytes: usize, val: u8) -> Result<(), Box<dyn core::error::Error>> {
		if !self.kasan_enabled { return Ok(()); }

		let shadow_start = mem_to_shadow(base).align_down_to_page();
		let shadow_end = mem_to_shadow(base + bytes).align_up_to_page();

		let mem = boot::allocate_pages(
			AllocateType::AnyPages,
			MemoryType::LOADER_DATA,
			(shadow_end.addr - shadow_start.addr).div_ceil(boot::PAGE_SIZE),
		)?;

		unsafe {
			ptr::write_bytes(
				mem.as_ptr(),
				0x00, // fixme: 0xc1,
				shadow_end.addr - shadow_start.addr,
			);

			ptr::write_bytes(
				mem.as_ptr().add(mem_to_shadow(base).addr - shadow_start.addr),
				val,
				count_to_shadow(bytes),
			)
		}

		for i in 0..(shadow_end - shadow_start) {
			let frame = RawFrame::new(mem.expose_provenance().get()) + i;
			if let Err(err) = self.page_table.try_map_range_with(
				shadow_start + i,
				frame,
				1,
				TableEntryFlags::GLOBAL | TableEntryFlags::WRITABLE | TableEntryFlags::NO_EXECUTE,
				Ty::SHADOW_MEM,
			) {
				if err.is::<AlreadyMappedError>() {
					warn!("leaking memory :(");
					// fixme: don't leak memory
					let mapped = self.page_table.translate_page(shadow_start + i)
						.expect("just got an already mapped error for this page");
					let current_val = unsafe { (mapped.addr as *mut u8).read() };
					if current_val == val { continue; }
					else { unimplemented!("{current_val:#x} != {val:#x}") }
				} else { return Err(err.into()); }
			} else {
				self.used_frames.push(frame);
			}
		}

		Ok(())
	}

	pub fn finalize(self) -> (PageTable, VirtualAddress, handoff::Stack, RawPage, &'static [RawFrame]) {
		let Self { page_table, entrypoint, stack, next_page, used_frames, .. } = self;
		(page_table, entrypoint, stack, next_page, Vec::leak(used_frames))
	}
}
