#[cfg(feature = "full")] pub use full::*;

use crate::newtype_enum;

newtype_enum! {
	/// The type of memory being mapped.
	///
	/// In certain build configurations this will be stored as metadata with each relevant page table
	/// entry, and will be shown in relevant log entries.
	///
	/// Constants are provided for typical memory buffer uses and will be printed by name in logs,
	/// but any custom value can be used and will have it's raw value printed.
	pub enum Ty: pub u8 => {
		/// Unknown.
		UNKNOWN = 0,
		/// Framebuffer memory.
		FB = 1,
		/// Kernel non-executable data.
		KERNEL_DATA = 2,
		/// Kernel executable code.
		KERNEL_CODE = 3,
		/// Kernel thread-local data.
		KERNEL_TLS = 4,
		/// Kernel thread stack.
		KERNEL_STACK = 6,
		/// Other kernel data.
		KERNEL_OTHER = 5,
		/// Memory used for the [page map region](`crate::memory#page-map-region`).
		MEM_MAP = 7,
		/// Bootloader executable code.
		LOADER_CODE = 8,
		/// Bootloader non-executable data.
		LOADER_DATA = 9,
		#[doc(hidden)] BGRT_BMP_HEADER = 10,
		#[doc(hidden)] IOAPIC_REGISTERS = 11,
		#[doc(hidden)] APIC_REGISTERS = 12,
		#[doc(hidden)] HPET_HEADER = 13,
		#[doc(hidden)] HPET_FULL = 14,
		#[doc(hidden)] PHYSMAP_OTHER = 15,
		#[doc(hidden)] ACPI_SDT_HEADER = 16,
		#[doc(hidden)] ACPI_RSDP = 17,
		#[doc(hidden)] ACPI_HPET = 18,
		#[doc(hidden)] ACPI_FADT = 19,
		#[doc(hidden)] ACPI_BGRT = 20,
		#[doc(hidden)] BYTE_ARRAY = 21,
		/// Userspace thread stack.
		USER_STACK = 24,
		/// Page table memory.
		PAGE_TABLE = 25,
		/// Userspace anonymous memory.
		USER_MMAP = 26,
		/// Userspace memory mapped to MMIO registers.
		USER_MMIO = 27,
		/// Userspace executable code and data.
		USER_CODE = 28,
		/// Memory used to buffer syscall data between processes.
		USER_PACKET_BUFFER = 29,
		/// [Kasan](`crate::memory::asan`) shadow memory.
		SHADOW_MEM = 30,
		/// Kernel heap memory.
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
	use crate::memory::{RawFrame, RawPage};
	use crate::syscall::handle::Handle;

	/// The protection that should be applied to the mapping.
	#[derive(Debug, Copy, Clone)]
	pub struct Protection {
		/// If the data in the mapping can be executed.
		pub executable: bool,
		/// If the mapping can be written to.
		pub writable: bool,
		/// If the mapping can be accessed from userspace.
		pub user_accessible: bool,
	}

	impl Protection {
		const fn new() -> Self {
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

	/// The caching strategy to use for the mapping.
	#[derive(Debug, Copy, Clone)]
	#[non_exhaustive]
	pub enum Caching {
		/// The fastest cache strategy.
		/// This should be used for the majority of mappings.
		Normal,
		/// All access bypasses the cache.
		/// This should only be used when preserving the exact sequence
		/// of memory accesses is important.
		Mmio,
		/// Reads bypass the cache. Small sequential writes are buffered
		/// to be written in a single large access.
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

	/// The strategy used to place the allocation within the address space.
	#[derive(Debug)]
	#[non_exhaustive]
	pub enum Location<T> {
		/// The memory can be allocated at any available address in the address space.
		Any,
		/// The allocator call should fail if the specifically requested address is unavailable.
		At(T),
	}

	#[derive(Debug)]
	enum AllocatorTy {
		Vmo(Arc<Handle>),
		Pmm(DynPmm<'static, false>),
	}

	/// The returned type when creating a userspace mapping.
	#[derive(Debug)]
	#[non_exhaustive]
	pub struct MappingMeta<T: DerefMut<Target = Mapping<Box<dyn Mappable + Send>, Userspace>>> {
		/// A key to access the mapping in future.
		pub key: MappingKey,
		/// An accessor to modify the mapping.
		pub mapping: T,
	}

	/// A [`Mapping`] builder, providing control over the way the underlying buffer is mapped.
	#[derive(Debug)]
	#[must_use = "mapping config does nothing unless mapped"]
	pub struct Config {
		physical_location: Location<RawFrame>,
		virtual_location: Location<RawPage>,
		page_count: NonZero<usize>,
		physical_allocator: AllocatorTy,
		protection: Protection,
		caching: Caching,
		reason: Ty,
	}

	impl Config {
		/// Creates a [`Mapping`] in the [kernel address space](address_space#kernel-address-space).
		///
		/// The type parameter `R` is used to determine how the physical and virtual allocations are aligned, as
		/// well as some configuration values if left as default values. This can be used to require extra virtual
		/// memory to be allocated and left unmapped to act as a guard page. See the [module level documentation](`super`)
		/// for more information.
		///
		/// # Errors
		///
		/// Propagates an [`AllocError`] if the virtual or physical memory allocators have an allocation failure.
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

		/// Creates a [`Mapping`] in a [user address space](address_space#user-address-space).
		///
		/// On success, returns a [`MappingMeta`] which allows access to the created mapping.
		///
		/// The type parameter `R` is used to determine how the physical and virtual allocations are aligned, as
		/// well as some configuration values if left as default values. This can be used to require extra virtual
		/// memory to be allocated and left unmapped to act as a guard page. See the [module level documentation](`super`)
		/// for more information.
		///
		/// # Errors
		///
		/// Propagates an [`AllocError`] if the virtual or physical memory allocators have an allocation failure.
		#[expect(clippy::type_complexity, reason = "cannot simplify further")]
		pub fn map_in<'a, R: Mappable + Default + Send + 'static>(self, name: Cow<'static, str>, address_space: &'a AddressSpace) -> Result<MappingMeta<impl DerefMut<Target = Mapping<Box<dyn Mappable + Send>, Userspace>> + 'a>, AllocError> {
			trace!("create mmap({}) with {self:#?}", core::any::type_name::<R>());
			let userspace = Userspace {
				inner: AddressSpace::clone(address_space),
			};
			let raw = R::default();

			let map = ManuallyDrop::new(self.do_map(raw, userspace)?);
			let map = Mapping {
				// SAFETY: pointer is derived directly from reference
				raw: Box::new(unsafe { ptr::read(&raw const map.raw) }) as Box<dyn Mappable + Send>,
				// SAFETY: pointer is derived directly from reference
				address_space: unsafe { ptr::read(&raw const map.address_space) },
				// SAFETY: pointer is derived directly from reference
				backing: unsafe { ptr::read(&raw const map.backing) },
				caching: map.caching,
				virtual_start: map.virtual_start,
				protection: map.protection,
			};

			let (key, mapping) = address_space.push_mapping(name, map);
			Ok(MappingMeta { key, mapping })
		}

		/// # Errors
		///
		/// Propagates an [`AllocError`] if the virtual or physical memory allocators have an allocation failure.
		fn do_map<R: Mappable, A: address_space::Ty>(self, raw: R, address_space: A) -> Result<Mapping<R, A>, AllocError> {
			let Self {
				physical_allocator,
				..
			} = self;

			let (base, backing) = match (physical_allocator, self.physical_location) {
				(AllocatorTy::Pmm(allocator), Location::At(frame)) => {
					let frames = allocator.allocate_at(frame, self.page_count)?;
					(frames.base(), Backing::Contiguous(frames))
				},
				(AllocatorTy::Pmm(allocator), Location::Any) => {
					let frames = allocator.allocate(self.page_count)?;
					(frames.base(), Backing::Contiguous(frames))
				},
				(AllocatorTy::Vmo(vmo), _) => {
					let base_addr = crate::bridge::handle::kernel_syscall_blocking(
						&vmo,
						6,
						1,
						[ 0 /* offset */, self.page_count.get() * 4096 /* len */, 0, 0 /* unused args */],
					)?;
					(RawFrame::new(base_addr as usize), Backing::Vmo { handle: vmo, frame_count: self.page_count.get() })
				}
			};

			let virtual_len = raw.virtual_size(self.page_count.get());
			let pages = match self.virtual_location {
				Location::At(frame) => address_space.allocator().allocate_contiguous_at(frame, virtual_len.get())?,
				Location::Any => address_space.allocator().allocate_contiguous(virtual_len.get())?,
			};
			let virtual_valid_start = pages + raw.base_virtual_offset();

			match address_space.map_contiguous(
				virtual_valid_start,
				base,
				self.page_count.get(),
				self.reason,
				self.protection,
				self.caching,
			) {
				Ok(()) => {}
				Err(MapPageError::AllocError) => return Err(AllocError::default()),
				Err(MapPageError::AlreadyMapped(ty)) => unreachable!("unallocated memory already allocated as {ty:?}"),
			}

			Ok(Mapping {
				raw,
				address_space: ManuallyDrop::new(address_space),
				backing: ManuallyDrop::new(backing),
				caching: self.caching,
				virtual_start: pages,
				protection: self.protection,
			})
		}

		/// Constructs a new `Config` for creating a [`Mapping`] with size `page_count` number of pages.
		///
		/// The following defaults are used:
		/// - Any physical memory location, including discontiguous memory
		/// - Any virtual memory location
		/// - [`highmem`](`crate::memory#highmem`) used to allocate physical memory
		/// - Read-only
		/// - Inaccessible from userspace
		/// - [`Normal`](`Caching::Normal`) cache strategy
		///
		/// Any unspecified defaults are not guaranteed, and should be explicitly specified
		/// with the relevant methods if specific behaviour is required.
		pub const fn new(page_count: NonZero<usize>, ty: Ty) -> Self {
			Self {
				physical_location: Location::Any,
				virtual_location: Location::Any,
				page_count,
				physical_allocator: AllocatorTy::Pmm(highmem().into()),
				protection: Protection::new(),
				caching: Caching::new(),
				reason: ty,
			}
		}

		/// Sets the caching method used for the mapping.
		///
		/// See [`Caching`] for more information.
		pub const fn caching(mut self, caching: Caching) -> Self {
			self.caching = caching;
			self
		}

		/// Sets the protection bits used when creating the mapping.
		pub const fn protection(mut self, writable: bool, executable: bool, user_accessible: bool) -> Self {
			self.protection = Protection { executable, writable, user_accessible };
			self
		}

		/// Uses the `core.mem.Pager` protocol on the `vmo` handle to supply the physical memory for the mapping.
		///
		/// This will cause the mapping to contain the contents of the VMO starting `offset` bytes into the VMO.
		/// If the mapping is writable, then any changes will be synced back to the VMO provider.
		///
		/// If memory access is attempted to the region backed by the VMO handle, and calling `core.mem.Pager` results
		/// in an error then this will appear as a page fault.
		///
		/// # Panics
		///
		/// If the `vmo` handle does not support the `core.mem.Pager` protocol, or if `offset` is not at least page aligned.
		pub fn with_vmo(self, vmo: Arc<Handle>, offset: usize) -> Self {
			assert!(vmo.has_protocols(&[6]), "vmo handle must support `core.mem.Pager`");
			assert!(offset.is_multiple_of(4096), "vmo offset must be page aligned");
			Self {
				physical_allocator: AllocatorTy::Vmo(vmo),
				.. self
			}
		}

		/// Uses `allocator` to supply the physical memory for the mapping.
		pub fn with_allocator<const RAM_ONLY: bool>(self, allocator: impl Into<DynPmm<'static, RAM_ONLY>>) -> Self {
			Self {
				physical_allocator: AllocatorTy::Pmm(allocator.into().into()),
				.. self
			}
		}

		/// Requires the mapping to be at a specific physical address.
		pub const fn physical_location(mut self, at: RawFrame) -> Self {
			self.physical_location = Location::At(at);
			self
		}

		/// Requires the mapping to be at a specific virtual address.
		pub const fn virtual_location(mut self, at: RawPage) -> Self {
			self.virtual_location = Location::At(at);
			self
		}
	}
}
