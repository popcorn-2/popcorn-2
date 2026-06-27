use alloc::borrow::Cow;
use alloc::sync::Arc;
use core::mem::ManuallyDrop;
use core::num::NonZero;
use core::ops::DerefMut;
use core::ptr;
use log::trace;
use crate::mapping::{Mappable, Mapping, Ty, Backing};
use crate::address_space::{self, MappingKey, MapPageError};
use crate::allocator::{highmem, AllocError, DynPmm};
use crate::memory::{RawFrame, RawPage, PAGE_SIZE};
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
#[derive(Debug, Copy, Clone, Default)]
#[non_exhaustive]
pub enum Caching {
	/// The fastest cache strategy.
	/// This should be used for the majority of mappings.
	#[default]
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
	const fn new() -> Self {
		Self::Normal
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
pub struct MappingMeta<T: DerefMut<Target = Mapping<Box<dyn Mappable + Send>, address_space::User>>> {
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

		let address_space = address_space::Kernel(());
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
	//#[cfg(not(feature = "use_std"))]
	#[expect(clippy::type_complexity, reason = "ca")]
	pub fn map_in<'a, R: Mappable + Default + Send + 'static>(self, name: Cow<'static, str>, address_space: &'a address_space::User) -> Result<MappingMeta<impl DerefMut<Target = Mapping<Box<dyn Mappable + Send>, address_space::User>> + 'a>, AllocError> {
		trace!("create mmap({}) with {self:#?}", core::any::type_name::<R>());
		let raw = R::default();

		let map = ManuallyDrop::new(self.do_map(raw, address_space::User::clone(address_space))?);
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
					[ 0 /* offset */, self.page_count.get() * PAGE_SIZE /* len */, 0, 0 /* unused args */],
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
			Err(MapPageError::AllocError(err)) => return Err(err),
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
		assert!(offset.is_multiple_of(PAGE_SIZE), "vmo offset must be page aligned");
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
