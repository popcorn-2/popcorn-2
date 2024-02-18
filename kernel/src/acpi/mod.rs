use core::fmt::{Debug, Formatter};
use core::mem::ManuallyDrop;
use core::num::NonZeroUsize;
use core::ops::{Deref, DerefMut, Div};
use core::ptr::{from_raw_parts, from_raw_parts_mut, NonNull, Pointee};
use acpi::{AcpiHandler, PhysicalMapping};
use log::debug;
use kernel_api::memory::mapping::{Config, Location, Mapping};
use kernel_api::memory::{Frame, Page, PhysicalAddress, VirtualAddress};
use kernel_api::memory::allocator::BackingAllocator;
use kernel_api::memory::physical::OwnedFrames;
use kernel_api::memory::r#virtual::{Global, OwnedPages};

#[derive(Copy, Clone)]
pub struct Handler<'allocator> {
	allocator: &'allocator dyn BackingAllocator
}

impl Debug for Handler<'_> {
	fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
		f.debug_struct("Handler")
				.finish()
	}
}

impl<'a> Handler<'a> {
	pub fn new(allocator: &'a dyn BackingAllocator) -> Handler<'a> {
		Self {
			allocator
		}
	}
}

impl Handler<'_> {
	pub unsafe fn map_region<T: ?Sized>(&self, physical_address: usize, size: usize, meta: <T as Pointee>::Metadata) -> XPhysicalMapping<Self, T> {
		debug!("physical_address = {physical_address:#x}, size = {size:#x}");
		// todo: clean up types here
		let lower_addr =  PhysicalAddress::<1>::new(physical_address).align_down();
		let offset = physical_address - lower_addr.addr;
		let upper_addr: PhysicalAddress<4096> = PhysicalAddress::<1>::new(physical_address + size).align_up();
		let actual_size = NonZeroUsize::new(upper_addr - lower_addr).expect("Cannot map zero size physical region");
		let page_count = unsafe { NonZeroUsize::new_unchecked(actual_size.get().div_ceil(4096)) };
		let config = Config::<Global>::new(page_count)
				.physical_location(Location::At(Frame::new(lower_addr)))
				.physical_allocator(self.allocator);
		let mapping = Mapping::new(config).expect("Unable to create physical mapping");


		let (frames, pages) = mapping.into_raw_parts();
		let (first_frame, phys_len, _) = frames.into_raw_parts();
		let (first_page, virt_len, _) = pages.into_raw_parts();
		assert_eq!(phys_len, virt_len);

		let start = unsafe { NonNull::new_unchecked(from_raw_parts_mut(first_page.as_ptr().add(offset).cast(), meta)) };
		XPhysicalMapping {
			physical_start: first_frame.start().addr + offset,
			virtual_start: start,
			region_length: size,
			mapped_length: phys_len.get() * 4096,
			handler: self.clone()
		}
	}
}

#[derive(Debug)]
pub struct XPhysicalMapping<A: AcpiHandler, T: ?Sized> {
	physical_start: usize,
	virtual_start: NonNull<T>,
	region_length: usize, // Can be equal or larger than size_of::<T>()
	mapped_length: usize, // Differs from `region_length` if padding is added for alignment
	handler: A,
}

impl<A: AcpiHandler, T: ?Sized> Deref for XPhysicalMapping<A, T> {
	type Target = T;

	fn deref(&self) -> &Self::Target {
		unsafe { self.virtual_start.as_ref() }
	}
}

impl<A: AcpiHandler, T: ?Sized> DerefMut for XPhysicalMapping<A, T> {
	fn deref_mut(&mut self) -> &mut Self::Target {
		unsafe { self.virtual_start.as_mut() }
	}
}

impl<A: AcpiHandler, T: ?Sized> Drop for XPhysicalMapping<A, T> {
	fn drop(&mut self) {
		let _drop_guard = unsafe {
			PhysicalMapping::new(
				self.physical_start,
				self.virtual_start.cast::<u8>(),
				self.region_length,
				self.mapped_length,
				self.handler.clone()
			)
		};
	}
}

impl AcpiHandler for Handler<'_> {
	unsafe fn map_physical_region<T>(&self, physical_address: usize, size: usize) -> PhysicalMapping<Self, T> {
		let xmap = self.map_region(physical_address, size, ());
		let xmap = ManuallyDrop::new(xmap);

		PhysicalMapping::new(
			xmap.physical_start,
			xmap.virtual_start,
			xmap.region_length,
			xmap.mapped_length,
			xmap.handler
		)
	}

	fn unmap_physical_region<T>(region: &PhysicalMapping<Self, T>) {
		unsafe {
			let first_frame = {
				let start = PhysicalAddress::<1>::new(region.physical_start());
				Frame::new(start.align_down())
			};
			let len = NonZeroUsize::new(region.mapped_length() / 4096).unwrap();
			let first_page = {
				let start = VirtualAddress::<1>::from(region.virtual_start().as_ptr());
				Page::new(start.align_down())
			};

			unsafe {
				let frames = OwnedFrames::from_raw_parts(first_frame, len, region.handler().allocator);
				let pages = OwnedPages::from_raw_parts(first_page, len, Global);
				let _mapping = Mapping::from_raw_parts(frames, pages);
			}
		}
	}
}
