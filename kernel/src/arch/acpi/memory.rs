use core::mem::{ManuallyDrop, MaybeUninit};
use core::num::NonZero;
use core::ptr::NonNull;
use acpi::{PciAddress, PhysicalMapping};
use kernel_api::allocator::{AllocError, DynPmm, Pmm};
use kernel_api::mapping::{Caching, Config, Mapping, Protection, UnsafeMmap};
use kernel_api::memory::{Frames, PhysicalAddress, RawFrame, PAGE_SIZE, RawPage};
use crate::hal::acpi::PagingReason;

pub fn handler() -> impl acpi::Handler {
	Handler
}

#[derive(Clone)]
struct Handler;

struct DummyAllocator;

unsafe impl Pmm<false> for DummyAllocator {
	fn allocate_raw(&self, _count: NonZero<usize>) -> Result<RawFrame, AllocError> { unimplemented!() }
	fn allocate_raw_at(&self, at: RawFrame, _count: NonZero<usize>) -> Result<RawFrame, AllocError> { Ok(at) }
	unsafe fn deallocate_raw(&self, _base: RawFrame, _count: NonZero<usize>) {}
}

impl acpi::Handler for Handler {
	unsafe fn map_physical_region<T>(&self, physical_address: usize, size: usize) -> PhysicalMapping<Self, T> {
		let physical_address = PhysicalAddress::new(physical_address);
		let base_frame = physical_address.align_down_to_frame();

		let offset_size = physical_address - *base_frame;
		let total_size = offset_size + size;
		let page_count = NonZero::new(total_size.div_ceil(PAGE_SIZE)).expect("cannot allocate ZST");

		let mmap = Config::new(page_count, <T as PagingReason>::reason())
			.caching(Caching::Mmio)
			.physical_location(base_frame)
			.with_allocator(DynPmm::from(&DummyAllocator))
			.protection(true, false, false)
			.map::<UnsafeMmap>()
			.expect("failed to create mapping");
		let (frames, base_page, _, _) = mmap.into_raw_parts();
		let frames = ManuallyDrop::new(frames.expect("default mapping should be contiguous"));

		PhysicalMapping {
			physical_start: physical_address.addr,
			virtual_start: NonNull::new(base_page.as_ptr().byte_add(offset_size))
				.expect("mmap returned nullptr")
				.cast(),
			region_length: size,
			mapped_length: frames.count(),
			handler: Handler,
		}
	}

	fn unmap_physical_region<T>(region: &PhysicalMapping<Self, T>) {
		let physical_address = PhysicalAddress::new(region.physical_start);
		let base_frame = physical_address.align_down_to_frame();
		let offset_size = physical_address - *base_frame;

		let frames = unsafe {
			let end_frame = base_frame + region.mapped_length;
			Frames::<_, MaybeUninit<u8>>::from_raw(
				base_frame..end_frame,
				DynPmm::from(&DummyAllocator)
			)
		};
		let mapping = unsafe {
			Mapping::<UnsafeMmap, _>::from_raw_parts(
				frames,
				RawPage::new(region.virtual_start.as_ptr().byte_sub(offset_size).addr()),
				Protection { executable: false, writable: true, user_accessible: false },
				Caching::Mmio,
			)
		};
		drop(mapping);
	}

	fn read_u8(&self, _address: usize) -> u8 { unimplemented!() }
	fn read_u16(&self, _address: usize) -> u16 { unimplemented!() }
	fn read_u32(&self, _address: usize) -> u32 { unimplemented!() }
	fn read_u64(&self, _address: usize) -> u64 { unimplemented!() }
	fn write_u8(&self, _address: usize, _value: u8) { unimplemented!() }
	fn write_u16(&self, _address: usize, _value: u16) { unimplemented!() }
	fn write_u32(&self, _address: usize, _value: u32) { unimplemented!() }
	fn write_u64(&self, _address: usize, _value: u64) { unimplemented!() }
	fn read_io_u8(&self, _port: u16) -> u8 { unimplemented!() }
	fn read_io_u16(&self, _port: u16) -> u16 { unimplemented!() }
	fn read_io_u32(&self, _port: u16) -> u32 { unimplemented!() }
	fn write_io_u8(&self, _port: u16, _value: u8) { unimplemented!() }
	fn write_io_u16(&self, _port: u16, _value: u16) { unimplemented!() }
	fn write_io_u32(&self, _port: u16, _value: u32) { unimplemented!() }
	fn read_pci_u8(&self, _address: PciAddress, _offset: u16) -> u8 { unimplemented!() }
	fn read_pci_u16(&self, _address: PciAddress, _offset: u16) -> u16 { unimplemented!() }
	fn read_pci_u32(&self, _address: PciAddress, _offset: u16) -> u32 { unimplemented!() }
	fn write_pci_u8(&self, _address: PciAddress, _offset: u16, _value: u8) { unimplemented!() }
	fn write_pci_u16(&self, _address: PciAddress, _offset: u16, _value: u16) { unimplemented!() }
	fn write_pci_u32(&self, _address: PciAddress, _offset: u16, _value: u32) { unimplemented!() }
	fn nanos_since_boot(&self) -> u64 { unimplemented!() }
	fn stall(&self, _microseconds: u64) { unimplemented!() }
	fn sleep(&self, _milliseconds: u64) { unimplemented!() }
}
