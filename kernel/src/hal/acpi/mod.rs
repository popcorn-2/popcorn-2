use core::fmt::{Debug, Formatter};
use core::mem::ManuallyDrop;
use core::num::NonZero;
use core::ops::{Deref, DerefMut};
use core::ptr::{from_raw_parts_mut, NonNull, Pointee};
use acpi::{AcpiTables, PciAddress, PhysicalMapping};
use kernel_api::sync::OnceLock;

static TABLES: OnceLock<AcpiTables<Handler>> = OnceLock::new();

#[track_caller]
pub fn tables() -> &'static AcpiTables<Handler> {
	TABLES.get().expect("ACPI tables not yet parsed")
}

pub unsafe fn init_tables(rsdp_addr: PhysicalAddress) {
	TABLES.get_or_init(|| {
		unsafe { AcpiTables::from_rsdp(Handler::new(&Allocator), rsdp_addr.addr) }
				.expect("Invalid ACPI table")
	});
}

pub use alloc::NullAllocator as Allocator;
use kernel_api::address_space;
use kernel_api::allocator::DynPmm;
use kernel_api::mapping::{Caching, Config, Mappable, Mapping, Mmap, Protection, Ty};
use kernel_api::memory::{Frames, PAGE_SIZE, PhysicalAddress, VirtualAddress};

mod alloc {
	use core::num::NonZero;
	use kernel_api::allocator::{AllocError, Pmm};
	use kernel_api::memory::RawFrame;

	pub struct NullAllocator;

	unsafe impl Pmm<false> for NullAllocator {
		fn allocate_raw(&self, _count: NonZero<usize>) -> Result<RawFrame, AllocError> {
			unimplemented!()
		}

		fn allocate_raw_at(&self, at: RawFrame, count: NonZero<usize>) -> Result<RawFrame, AllocError> {
			trace!("=== aml a {:#018x} -> {:#018x}", at, at + count.get());
			Ok(at)
		}

		unsafe fn deallocate_raw(&self, _base: RawFrame, _count: NonZero<usize>) {}
	}
}

#[derive(Copy, Clone)]
pub struct Handler {
	allocator: DynPmm<'static, false>,
}

impl Debug for Handler {
	fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
		f.debug_struct("Handler")
				.finish()
	}
}

impl Handler {
	pub fn new(allocator: impl Into<DynPmm<'static, false>>) -> Handler {
		Self {
			allocator: allocator.into()
		}
	}
}

pub trait PagingReason {
	fn reason() -> Ty;
}

impl<T: ?Sized> PagingReason for T {
	default fn reason() -> Ty {
		Ty::PHYSMAP_OTHER
	}
}

impl PagingReason for acpi::sdt::SdtHeader {
	fn reason() -> Ty {
		Ty::ACPI_SDT_HEADER
	}
}

impl PagingReason for acpi::rsdp::Rsdp {
	fn reason() -> Ty {
		Ty::ACPI_RSDP
	}
}

impl PagingReason for acpi::sdt::hpet::HpetTable {
	fn reason() -> Ty {
		Ty::ACPI_HPET
	}
}

impl PagingReason for acpi::sdt::fadt::Fadt {
	fn reason() -> Ty {
		Ty::ACPI_FADT
	}
}

impl PagingReason for acpi::sdt::bgrt::Bgrt {
	fn reason() -> Ty {
		Ty::ACPI_BGRT
	}
}

impl PagingReason for [u8] {
	fn reason() -> Ty {
		Ty::BYTE_ARRAY
	}
}

impl AcpiHandlerExt for Handler {
	unsafe fn map_region<T: ?Sized>(&self, physical_address: PhysicalAddress, size: usize, meta: <T as Pointee>::Metadata) -> XPhysicalMapping<Self, T> {
		debug!("physical_address = {physical_address:#x}, size = {size:#x}");
		let lower_addr = physical_address.align_down_to_frame();
		let offset = physical_address - *lower_addr;
		let upper_addr = (physical_address + size).align_up_to_frame();
		
		let page_count = NonZero::<usize>::new(upper_addr - lower_addr).expect("Cannot map zero size physical region");
		
		let mapping = Config::new(page_count, T::reason())
				.protection(true, false, false)
				.caching(Caching::Mmio)
				.physical_location(lower_addr)
				.with_allocator(self.allocator)
				.map::<Mmap>()
				.unwrap();

		let virtual_base = mapping.virtual_valid_start();
		let (Some(frames), _, _, _) = mapping.into_raw_parts() else {
			unreachable!("initial allocation must be contiguous")
		};

		let (first_frame, phys_len) = (frames.base(), frames.count());
		core::mem::forget(frames);

		let start = unsafe { NonNull::new_unchecked(from_raw_parts_mut(virtual_base.as_ptr().add(offset), meta)) };
		XPhysicalMapping {
			physical_start: first_frame.addr + offset,
			virtual_start: start,
			region_length: size,
			mapped_length: phys_len * PAGE_SIZE,
			handler: self.clone()
		}
	}
}

#[derive(Debug)]
pub struct XPhysicalMapping<A: acpi::Handler, T: ?Sized> {
	physical_start: usize,
	pub(crate) virtual_start: NonNull<T>,
	region_length: usize, // Can be equal or larger than size_of::<T>()
	mapped_length: usize, // Differs from `region_length` if padding is added for alignment
	handler: A,
}

impl<A: acpi::Handler, T: ?Sized> Deref for XPhysicalMapping<A, T> {
	type Target = T;

	fn deref(&self) -> &Self::Target {
		unsafe { self.virtual_start.as_ref() }
	}
}

impl<A: acpi::Handler, T: ?Sized> DerefMut for XPhysicalMapping<A, T> {
	fn deref_mut(&mut self) -> &mut Self::Target {
		unsafe { self.virtual_start.as_mut() }
	}
}

pub trait AcpiHandlerExt: acpi::Handler {
	unsafe fn map_region<T: ?Sized>(&self, physical_address: PhysicalAddress, size: usize, meta: <T as Pointee>::Metadata) -> XPhysicalMapping<Self, T>;
}

impl<A: acpi::Handler, T: ?Sized> Drop for XPhysicalMapping<A, T> {
	fn drop(&mut self) {
		let _drop_guard = PhysicalMapping {
			physical_start: self.physical_start,
			virtual_start: self.virtual_start.cast::<u8>(),
			region_length: self.region_length,
			mapped_length: self.mapped_length,
			handler: self.handler.clone()
		};
	}
}

impl acpi::Handler for Handler {
	unsafe fn map_physical_region<T>(&self, physical_address: usize, size: usize) -> PhysicalMapping<Self, T> {
		let physical_address = PhysicalAddress::new(physical_address);
		let xmap = unsafe { self.map_region(physical_address, size, ()) };
		let xmap = ManuallyDrop::new(xmap);

		PhysicalMapping {
			physical_start: xmap.physical_start,
			virtual_start: xmap.virtual_start,
			region_length: xmap.region_length,
			mapped_length: xmap.mapped_length,
			handler: xmap.handler,
		}
	}

	fn unmap_physical_region<T>(region: &PhysicalMapping<Self, T>) {
		let first_frame = {
			let start = PhysicalAddress::new(region.physical_start);
			start.align_down_to_frame()
		};
		let len = NonZero::<usize>::new(region.mapped_length / PAGE_SIZE).unwrap();
		let first_page = {
			let start = VirtualAddress::from(region.virtual_start.as_ptr());
			start.align_down_to_page() + -Mmap::default().base_virtual_offset()
		};

		unsafe {
			let frames = Frames::<false>::from_raw(
				first_frame .. (first_frame + len.get()),
				region.handler.allocator,
			);
			
			let _ = Mapping::<Mmap, address_space::Kernel>::from_raw_parts(
				frames,
				first_page,
				Protection { executable: false, writable: true, user_accessible: false },
				Caching::Mmio,
			);
		}
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
