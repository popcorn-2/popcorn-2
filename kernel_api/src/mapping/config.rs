#[cfg(feature = "full")] pub use full::*;

use crate::newtype_enum;

newtype_enum! {
	pub enum Ty: u8 => {
		FB = 1,
		KERNEL_DATA = 2,
		KERNEL_CODE = 3,
		KERNEL_TLS = 4,
		KERNEL_OTHER = 5,
		KERNEL_STACK = 6,
		MEM_MAP = 7,
		LOADER_CODE = 8,
		LOADER_DATA = 9,
		BGRT_BMP_HEADER = 10,
		IOAPIC_REGISTERS = 11,
		APIC_REGISTERS = 12,
		HPET_HEADER = 13,
		HPET_FULL = 14,
		PHYSMAP_OTHER = 15,
		ACPI_SDT_HEADER = 16,
		ACPI_RSDP = 17,
		ACPI_HPET = 18,
		ACPI_FADT = 19,
		ACPI_BGRT = 20,
		BYTE_ARRAY = 21,
		THREAD_KERNEL_STACK = 22,
		TLS = 23,
		USER_STACK = 24,
		PAGE_TABLE = 25,
		USER_MMAP = 26,
		USER_MMIO = 27,
		USER_CODE = 28,
		USER_PACKET_BUFFER = 29,
		SHADOW_MEM = 30,
		HEAP = 31,
	}
}

#[cfg(feature = "full")]
mod full {
	use alloc::borrow::Cow;
	use alloc::boxed::Box;
	use alloc::sync::Arc;
	use core::mem::ManuallyDrop;
	use core::num::NonZero;
	use core::ops::DerefMut;
	use core::ptr;
	use log::trace;
	use crate::mapping::{MapPageError, Mappable, Mapping, Ty};
	use crate::address_space;
	use crate::address_space::{AddressSpace, MappingKey, Userspace};
	use crate::allocator::{highmem, AllocError, DynPmm};
	use crate::mapping::full::Backing;
	use crate::memory::{RawFrame, RawPage, PAGE_SIZE};
	use crate::syscall::handle::Handle;

	#[derive(Debug, Copy, Clone)]
	pub struct Protection {
		pub executable: bool,
		pub writable: bool,
		pub user_accessible: bool,
	}

	impl Protection {
		pub const fn new() -> Self {
			Self {
				executable: false,
				writable: false,
				user_accessible: false
			}
		}
	}

	impl Default for Protection {
		fn default() -> Self {
			Self::new()
		}
	}

	#[derive(Debug, Copy, Clone)]
	pub enum Caching {
		Normal,
		Mmio,
		WriteCombine,
	}

	impl Caching {
		pub const fn new() -> Self {
			Self::Normal
		}
	}

	impl Default for Caching {
		fn default() -> Self {
			Self::new()
		}
	}

	#[derive(Debug)]
	pub enum Location<T> {
		Any,
		At(T),
	}

	#[derive(Debug)]
	pub struct Config {
		physical_location: Location<RawFrame>,
		virtual_location: Location<RawPage>,
		page_count: NonZero<usize>,
		physical_allocator: DynPmm<'static, false>,
		protection: Protection,
		caching: Caching,
		reason: Ty,
	}

	impl Config {
		//#[cfg(not(feature = "use_std"))]
		pub fn map<R: Mappable + Default>(self) -> Result<Mapping<R, address_space::Kernel>, AllocError> {
			trace!("create mmap({}) with {self:#?}", core::any::type_name::<R>());

			let address_space = address_space::Kernel {};
			let raw = R::default();

			self.do_map(raw, address_space)
		}

		/*#[cfg(feature = "use_std")]
		pub fn map<R: Mappable + Default>(self) -> Result<Mapping<R, address_space::Kernel>, AllocError> {
			trace!("create mmap({}) with {self:#?}", core::any::type_name::<R>());

			let address_space = address_space::Kernel {};
			let raw = R::default();

			let Location::Any = self.physical_location else {
				panic!("specific location not supported under test yet");
			};
			let Location::Any = self.virtual_location else {
				panic!("specific location not supported under test yet");
			};

			let virtual_len = raw.virtual_size(self.page_count.get());

			let mut prot = libc::PROT_READ;
			if self.protection.writable { prot |= libc::PROT_WRITE; }
			if self.protection.executable { prot |= libc::PROT_EXEC; }

			let base = unsafe {
				let res = libc::mmap(
					ptr::null_mut(),
					virtual_len.get() * PAGE_SIZE,
					prot,
					libc::MAP_ANON,
					0,
					0
				);
				if res.is_null() { return Err(AllocError::default()); }
				res
			};

			let guard_below = raw.base_virtual_offset();
			unsafe {
				libc::mprotect(
					base,
					(guard_below as usize) * PAGE_SIZE,
					libc::PROT_NONE,
				);
			}
			let guard_above = virtual_len.get() - self.page_count.get() - (guard_below as usize);
			unsafe {
				libc::mprotect(
					base.byte_add(((guard_below as usize) + self.page_count.get()) * PAGE_SIZE),
					(guard_above as usize) * PAGE_SIZE,
					libc::PROT_NONE,
				);
			}

			Ok(Mapping {
				raw,
				address_space: ManuallyDrop::new(address_space),
				backing: base,
				caching: self.caching,
				protection: self.protection,
			})
		}*/

		//#[cfg(not(feature = "use_std"))]
		pub fn map_in<'a, R: Mappable + Default + Send + 'static>(self, name: Cow<'static, str>, address_space: &'a AddressSpace) -> Result<(MappingKey, impl DerefMut<Target = Mapping<Box<dyn Mappable + Send>, Userspace>> + 'a), AllocError> {
			trace!("create mmap({}) with {self:#?}", core::any::type_name::<R>());
			let userspace = Userspace {
				inner: AddressSpace::clone(address_space),
			};
			let raw = R::default();

			let map = ManuallyDrop::new(self.do_map(raw, userspace)?);
			let map = Mapping {
				raw: Box::new(unsafe { ptr::read(&map.raw) }) as Box<dyn Mappable + Send>,
				address_space: unsafe { ptr::read(&map.address_space) },
				backing: unsafe { ptr::read(&map.backing) },
				caching: map.caching,
				virtual_start: map.virtual_start,
				protection: map.protection,
			};

			Ok(address_space.push_mapping(name, map))
		}

		//#[cfg(not(feature = "use_std"))]
		fn do_map<R: Mappable, A: address_space::Ty>(self, raw: R, address_space: A) -> Result<Mapping<R, A>, AllocError> {
			let frames = match self.physical_location {
				Location::At(frame) => self.physical_allocator.allocate_at(frame, self.page_count)?,
				Location::Any => self.physical_allocator.allocate(self.page_count)?,
			};

			let virtual_len = raw.virtual_size(self.page_count.get());
			let pages = match self.virtual_location {
				Location::At(frame) => address_space.allocator().allocate_contiguous_at(frame, virtual_len.get())?,
				Location::Any => address_space.allocator().allocate_contiguous(virtual_len.get())?,
			};
			let virtual_valid_start = pages + raw.base_virtual_offset();

			match address_space.map_contiguous(
				virtual_valid_start,
				frames.base(),
				self.page_count.get(),
				self.reason,
				self.protection,
				self.caching,
			) {
				Ok(_) => {}
				Err(MapPageError::AllocError) => return Err(AllocError::default()),
				Err(MapPageError::AlreadyMapped(ty)) => unreachable!("unallocated memory already allocated as {ty:?}"),
			}

			Ok(Mapping {
				raw,
				address_space: ManuallyDrop::new(address_space),
				backing: Backing::Contiguous(frames),
				caching: self.caching,
				virtual_start: pages,
				protection: self.protection,
			})
		}

		pub const fn new(page_count: NonZero<usize>, ty: Ty) -> Self {
			Self {
				physical_location: Location::Any,
				virtual_location: Location::Any,
				page_count,
				physical_allocator: DynPmm::from(highmem()).into(),
				protection: Protection::new(),
				caching: Caching::new(),
				reason: ty,
			}
		}

		pub const fn caching(self, caching: Caching) -> Self {
			Self {
				caching,
				.. self
			}
		}

		pub const fn protection(self, writable: bool, executable: bool, user_accessible: bool) -> Self {
			Self {
				protection: Protection { writable, executable, user_accessible },
				.. self
			}
		}

		pub fn with_vmo(self, vmo: Arc<Handle>, ) -> Self {
			assert!(vmo.has_protocols(&[6]), "vmo handle must support `core.mem.Pager`");
			todo!()
		}

		pub fn with_allocator<const RAM_ONLY: bool>(self, allocator: impl Into<DynPmm<'static, RAM_ONLY>>) -> Self {
			Self {
				physical_allocator: allocator.into().into(),
				.. self
			}
		}

		pub const fn physical_location(self, at: RawFrame) -> Self {
			Self {
				physical_location: Location::At(at),
				.. self
			}
		}

		pub const fn virtual_location(self, at: RawPage) -> Self {
			Self {
				virtual_location: Location::At(at),
				.. self
			}
		}
	}
}