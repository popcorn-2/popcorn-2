//! API for managing memory at a high level.
//!
//! The mapping API implements a RAII based API for managing memory maps. Each memory map owns a region of physical
//! and virtual memory, and manages the paging required to map the two together. It is also possible for only a subset
//! of the virtual memory region to be mapped to physical memory.
//!
//! Each map is built on a [`Mappable`] type, which implements the required methods to calculate the required virtual
//! memory, and how to map it to physical memory. This can be used to instantiate a [`RawMapping`] which handles the
//! actual mapping.
//!
//! This module exports two common flavours of memory map: [`Mapping`] and [`Stack`].

#![stable(feature = "kernel_mmap", since = "1.1.0")]

use core::fmt::{Debug, Formatter};
use core::marker::PhantomData;
use core::mem::ManuallyDrop;
use core::num::NonZero;
use log::debug;
use crate::memory::allocator::{PhysicalAllocator, SpecificLocation};
use crate::memory::{AllocError, Frame, Page};
use crate::memory::physical::{OwnedFrames, highmem};
use crate::memory::r#virtual::{Global, OwnedPages, VirtualAllocator};

/// Basic operations to decide how to map memory together.
///
/// Implementations of this can be used to instantiate a [`RawMapping`].
#[unstable(feature = "kernel_mmap_trait", issue = "24")]
pub trait Mappable {
	/// The amount of virtual memory required to create a mapping with `physical_length` [`Frame`]s
	fn physical_length_to_virtual_length(physical_length: NonZero<usize>) -> NonZero<usize>;

	/// The number of [`Page`]s to offset the physical memory into the allocated virtual memory
	fn physical_start_offset_from_virtual() -> isize;
}

/// The memory protection to use for the memory mapping
#[unstable(feature = "kernel_mmap_config", issue = "24")]
pub enum Protection {
	/// The mapping is read-write and can be executed from
	RWX
}

mod private {
	use crate::memory::{Frame, Page};

	#[unstable(feature = "kernel_mmap_config", issue = "24")]
	pub trait Sealed {}

	#[unstable(feature = "kernel_mmap_config", issue = "24")]
	impl Sealed for Page {}
	#[unstable(feature = "kernel_mmap_config", issue = "24")]
	impl Sealed for Frame {}
}

/// A marker trait for types that can be used as a [`Location`]
#[unstable(feature = "kernel_mmap_config", issue = "24")]
pub trait Address: private::Sealed {}
#[unstable(feature = "kernel_mmap_config", issue = "24")]
impl Address for Page {}
#[unstable(feature = "kernel_mmap_config", issue = "24")]
impl Address for Frame {}

/// The location at which to make the [mapping](self)
#[stable(feature = "kernel_mmap", since = "1.1.0")]
pub enum Location<A: Address> {
	/// The mapping can go anywhere
	#[stable(feature = "kernel_mmap", since = "1.1.0")] Any,
	/// The mapping must be aligned to a specific number of [`Page`]s/[`Frame`]s
	#[unstable(feature = "kernel_mmap_config", issue = "24")] Aligned(NonZero<u32>),
	/// The mapping will fail if it cannot be allocated at this exact location
	#[stable(feature = "kernel_mmap", since = "1.1.0")] At(#[stable(feature = "kernel_mmap", since = "1.1.0")] A),
	/// The mapping must be below this location, aligned to `with_alignment` number of [`Page`]s/[`Frame`]s
	#[unstable(feature = "kernel_mmap_config", issue = "24")] Below { location: A, with_alignment: NonZero<u32> }
}

#[doc(hidden)]
#[unstable(feature = "kernel_mmap_config", issue = "24")]
impl From<Location<Frame>> for super::allocator::Location {
	fn from(value: Location<Frame>) -> Self {
		use super::allocator::{Location as XLocation, SpecificLocation};
		match value {
			Location::Any => XLocation::Any,
			Location::Aligned(a) => XLocation::Specific(SpecificLocation::Aligned(a)),
			Location::At(f) => XLocation::Specific(SpecificLocation::At(f)),
			Location::Below { location, with_alignment } => XLocation::Specific(SpecificLocation::Below{ location, with_alignment }),
		}
	}
}

/// When to allocate physical memory for the [mapping](self)
#[unstable(feature = "kernel_mmap_config", issue = "24")]
pub enum Laziness { Lazy, Prefault }

/// Configuration for creating a [mapping](self)
///
/// By default, it will allocate memory anywhere that is valid, using the kernel [`AddressSpace`], and the
/// `highmem` [`physical allocator`](PhysicalAllocator). It will lazily allocate physical memory, and map it
/// with read, write and execute permissions.
#[stable(feature = "kernel_mmap", since = "1.1.0")]
pub struct Config<'physical_allocator, A: VirtualAllocator> {
	physical_location: Location<Frame>,
	_virtual_location: Location<Page>,
	_laziness: Laziness,
	length: NonZero<usize>,
	physical_allocator: &'physical_allocator dyn PhysicalAllocator,
	virtual_allocator: A,
	_protection: Protection,
}

impl Config<'static, Global> {
	/// Creates a new [mapping](self) configuration with default options
	///
	/// `length` is specified in pages
	///
	/// The default options are not guaranteed, but at the moment are:
	/// - physical and virtual locations: anywhere
	/// - lazily allocated
	/// - Highmem physical allocator
	/// - Kernel address space
	/// - Readable, writable and executable
	#[stable(feature = "kernel_mmap", since = "1.1.0")]
	pub fn new(length: NonZero<usize>) -> Self {
		Config {
			physical_location: Location::Any,
			_virtual_location: Location::Any,
			_laziness: Laziness::Lazy,
			length,
			physical_allocator: highmem(),
			virtual_allocator: Global,
			_protection: Protection::RWX,
		}
	}
}

impl<'physical_allocator, A: VirtualAllocator> Config<'physical_allocator, A> {
	/// Set the physical allocator to use
	///
	/// This is used for both the underlying memory and any page tables that need creating
	#[unstable(feature = "kernel_mmap_config", issue = "24")]
	pub fn physical_allocator<'a>(self, allocator: &'a dyn PhysicalAllocator) -> Config<'a, A> {
		Config {
			physical_allocator: allocator,
			.. self
		}
	}

	/// Set the virtual allocator to use
	#[unstable(feature = "kernel_mmap_config", issue = "24")]
	pub fn virtual_allocator<New: VirtualAllocator>(self, allocator: New) -> Config<'physical_allocator, New> {
		Config {
			virtual_allocator: allocator,
			.. self
		}
	}

	#[unstable(feature = "kernel_mmap_config", issue = "24")]
	pub fn protection(self, protection: Protection) -> Self {
		Config {
			_protection: protection,
			.. self
		}
	}

	/// Set the physical location of the low address of the mapping
	#[stable(feature = "kernel_mmap", since = "1.1.0")]
	pub fn physical_location(self, location: Location<Frame>) -> Self {
		Config {
			physical_location: location,
			.. self
		}
	}

	/// Set the virtual location of the low address of the mapping
	///
	/// This is currently ignored
	#[unstable(feature = "kernel_mmap_config", issue = "24")]
	pub fn virtual_location(self, location: Location<Page>) -> Self {
		Config {
			_virtual_location: location,
			.. self
		}
	}
}

/// Used to track if the memory underlying the mapping is contiguous
pub(super) enum RawMappingContiguity {
	/// The underlying physical memory is contiguous, and starts at the contained frame
	Contiguous(Frame),

	/// The underlying physical memory is discontiguous, but all allocated by the same
	Discontiguous,
}

/// Returned from [`RawMapping::into_contiguous_raw_parts()`] if the underlying physical memory is not contiguous
///
/// See the documentation for [`into_contiguous_raw_parts()`](RawMapping::into_contiguous_raw_parts()) for more information.
#[derive(Debug)]
#[unstable(feature = "kernel_mmap_to_parts", issue = "24")]
pub struct DiscontiguityError(());

/// The raw type underlying all memory mappings.
///
/// This will allocate any required memory when created, and register any lazily mapped memory as such.
/// It will also manage the page tables to correctly unmap the memory when dropped.
#[stable(feature = "kernel_mmap", since = "1.1.0")]
pub struct RawMapping<'phys_allocator, R: Mappable, A: VirtualAllocator> {
	raw: PhantomData<R>,

	/// The virtual allocator used for memory allocation
	virtual_allocator: ManuallyDrop<A>,

	/// Whether the underlying physical memory is contiguous or not
	contiguity: RawMappingContiguity,

	/// The first page in the mapping that is mapped to physical memory.
	/// The region of virtual memory from `virtual_valid_start` to `virtual_valid_start + physical_length` is mapped.
	virtual_valid_start: Page,
	
	/// The number of physical pages allocated to the mapping
	physical_len: NonZero<usize>,

	/// The physical allocator used for memory allocation
	allocator: &'phys_allocator dyn PhysicalAllocator
}

#[stable(feature = "kernel_mmap", since = "1.1.0")]
impl<R: Mappable, A: VirtualAllocator> Debug for RawMapping<'_, R, A> {
	fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
		f.debug_struct("RawMapping")
		 .field(
			 "physical_base",
			 match self.physical_start() {
				 Ok(ref frame) => frame,
				 Err(_) => &"<discontiguous>",
			 }
		 )
		 .field("physical_length", &self.physical_len().get())
		 .field("virtual_base", &self.virtual_start())
		 .field("virtual_valid_start", &self.virtual_valid_start())
		 .field("virtual_allocator", &"<virtual allocator>")
		 .finish()
	}
}

impl<'phys_alloc, R: Mappable, A: VirtualAllocator> RawMapping<'phys_alloc, R, A> {
	/// Create a new memory mapping with the given configuration
	///
	/// All physical memory used for the initial allocation will be contiguous.
	/// This may change in future.
	///
	/// # Errors
	///
	/// If the required physical or virtual memory could not be allocation, [`AllocError`] is returned.
	///
	/// # Panics
	///
	/// If the page tables already contained a mapping for the newly allocated virtual memory.
	#[stable(feature = "kernel_mmap", since = "1.1.0")]
	pub fn new(config: Config<'phys_alloc, A>, reason: u16) -> Result<Self, AllocError> {
		let Config { length, physical_allocator, virtual_allocator, physical_location, .. } = config;

		let virtual_len = R::physical_length_to_virtual_length(length);
		let physical_len = length;

		let physical_mem = OwnedFrames::xnew(physical_len, physical_allocator, physical_location.into())?;
		let virtual_mem = OwnedPages::new_with(virtual_len, virtual_allocator)?;

		let (physical_base, _, _) = physical_mem.into_raw_parts();
		let (virtual_base, _, virtual_allocator) = virtual_mem.into_raw_parts();
		let offset_base = virtual_base + R::physical_start_offset_from_virtual();

		// TODO: huge pages
		// FIXME: memory leak of physical and virtual memory if this fails
		// fixme: can't assume ktable depending on AddressSpace once #43 is sorted
		let mut page_table = unsafe { crate::bridge::paging::__popcorn_paging_get_ktable() };
		for (frame, page) in (0..physical_len.get()).map(|i| (physical_base + i, offset_base + i)) {
			unsafe { crate::bridge::paging::__popcorn_paging_ktable_map_page(&mut page_table, page, frame, reason) }
					.expect("Virtual memory uniquely owned by the allocation so should not be mapped in this address space");
		}

		Ok(Self {
			raw: PhantomData,
			virtual_allocator: ManuallyDrop::new(virtual_allocator),
			contiguity: RawMappingContiguity::Contiguous(physical_base),
			virtual_valid_start: offset_base,
			physical_len,
			allocator: physical_allocator,
		})
	}

	/// Destructure into the underlying [`OwnedFrames`] and [`OwnedPages`] that back the allocation
	///
	/// Depending on the implementation of [`Mappable`] used, these may be different length.
	/// These can be turned back into a [`RawMapping`] by calling [`from_raw_parts()`].
	///
	/// # Errors
	///
	/// If the underlying physical memory is not contiguous, and so cannot be represented as a single instance
	/// of [`OwnedFrames`], [`DiscontiguityError`] is returned.
	#[unstable(feature = "kernel_mmap_to_parts", issue = "24")]
	pub fn into_contiguous_raw_parts(mut self) -> Result<(OwnedFrames<'phys_alloc>, OwnedPages<A>), DiscontiguityError> {
		let frames = unsafe {
			let RawMappingContiguity::Contiguous(base_frame) = self.contiguity else {
				return Err(DiscontiguityError(()));
			};
			
			OwnedFrames::from_raw_parts(
				base_frame,
				self.physical_len(),
				self.allocator,
			)
		};
		
		let virtual_allocator = unsafe { ManuallyDrop::take(&mut self.virtual_allocator) };
		let pages = unsafe {
			OwnedPages::from_raw_parts(
				self.virtual_start(),
				self.virtual_len(),
				virtual_allocator
			)
		};

		core::mem::forget(self);
		Ok((frames, pages))
	}

	#[unstable(feature = "kernel_mmap_to_parts", issue = "24")]
	pub unsafe fn from_contiguous_raw_parts(frames: OwnedFrames<'phys_alloc>, pages: OwnedPages<A>) -> Self {
		let (virtual_base, actual_vlen, virtual_allocator) = pages.into_raw_parts();
		let (physical_base, physical_len, physical_allocator) = frames.into_raw_parts();
		let correct_vlen = R::physical_length_to_virtual_length(physical_len);
		debug_assert_eq!(actual_vlen, correct_vlen);

		Self {
			raw: PhantomData,
			virtual_allocator: ManuallyDrop::new(virtual_allocator),
			contiguity: RawMappingContiguity::Contiguous(physical_base),
			virtual_valid_start: virtual_base + R::physical_start_offset_from_virtual(),
			physical_len,
			allocator: physical_allocator,
		}
	}

	fn virtual_len(&self) -> NonZero<usize> {
		R::physical_length_to_virtual_length(self.physical_len())
	}

	#[unstable(feature = "kernel_mmap_to_parts", issue = "24")]
	pub fn virtual_start(&self) -> Page {
		self.virtual_valid_start() - R::physical_start_offset_from_virtual()
	}

	fn virtual_valid_start(&self) -> Page {
		self.virtual_valid_start
	}

	#[unstable(feature = "kernel_mmap_to_parts", issue = "24")]
	pub fn virtual_end(&self) -> Page {
		self.virtual_start() + self.virtual_len().get()
	}

	#[unstable(feature = "kernel_mmap_to_parts", issue = "24")]
	pub fn physical_len(&self) -> NonZero<usize> {
		self.physical_len
	}

	#[unstable(feature = "kernel_mmap_to_parts", issue = "24")]
	pub fn physical_start(&self) -> Result<Frame, DiscontiguityError> {
		match self.contiguity {
			RawMappingContiguity::Contiguous(base_frame) => Ok(base_frame),
			RawMappingContiguity::Discontiguous => Err(DiscontiguityError(())),
		}
	}

	#[unstable(feature = "kernel_mmap_to_parts", issue = "24")]
	pub fn physical_end(&self) -> Result<Frame, DiscontiguityError> {
		match self.contiguity {
			RawMappingContiguity::Contiguous(base_frame) => Ok(base_frame + self.physical_len().get()),
			RawMappingContiguity::Discontiguous => Err(DiscontiguityError(())),
		}
	}

	/// Attempts to resize the allocation to `new_len` without moving the allocation
	/// 
	/// If the allocation could be resized, the [`Page`] corresponding to the previous end of the mapping.
	/// If it could not be resized, it returns the [`AllocError`] from the underlying allocators.
	#[stable(feature = "kernel_mmap", since = "1.1.0")]
	pub fn resize_in_place(&mut self, new_len: NonZero<usize>) -> Result<Page, AllocError> {
		if new_len == self.physical_len() { return Ok(self.virtual_end()); }

		let original_physical_allocator = self.allocator;

		if new_len < self.physical_len() {
			todo!("actually free and unmap the extra memory")
		} else {
			let extra_len: NonZero<usize> = new_len.get().checked_sub(self.physical_len().get())
			                             .expect("`new_len` is checked to be greater than `physical_len`")
										.try_into()
										.expect("`new_len` is checked to be not equal to `physical_len`");

			let extra_virtual_mem = Global.allocate_contiguous_at(self.virtual_end(), extra_len.get())?; // FIXME: use OwnedPages

			debug_assert_eq!(self.virtual_end(), extra_virtual_mem);
			
			// TODO: huge pages
			let mut page_table = unsafe { crate::bridge::paging::__popcorn_paging_get_ktable() };

			let extra_physical_mem = OwnedFrames::xnew(extra_len, original_physical_allocator, super::allocator::Location::Any)?;
			let (new_start_frame, _, _) = extra_physical_mem.into_raw_parts();

			if let RawMappingContiguity::Contiguous(base_frame) = self.contiguity {
				if base_frame + self.physical_len.get() != base_frame { self.contiguity = RawMappingContiguity::Discontiguous; }
			}

			// FIXME: memory leak of physical and virtual memory if this fails
			// FIXME: can't assume ktable depending on AddressSpace once #43 is sorted
			// FIXME: this is probably wrong if the extra unmapped virtual memory is after the physical memory, not before
			for (frame, page) in (0..extra_len.get()).map(|i| (new_start_frame + i, extra_virtual_mem + i)) {
				unsafe { crate::bridge::paging::__popcorn_paging_ktable_map_page(&mut page_table, page, frame, 25) }
						.expect("todo");
			}

			self.physical_len = new_len;
			Ok(extra_virtual_mem)
		}
	}
}

#[stable(feature = "kernel_mmap", since = "1.1.0")]
impl<R: Mappable, A: VirtualAllocator> Drop for RawMapping<'_, R, A> {
	fn drop(&mut self) {
		debug!("mmap dropped: {self:x?}");

		// fixme: can't assume ktable depending on AddressSpace once #43 is sorted
		let mut page_table = unsafe { crate::bridge::paging::__popcorn_paging_get_ktable() };

		// If the underlying memory is discontiguous, we need to find the physical memory chunks via the page tables
		// and deallocate them before unmapping. To reduce the number of allocator calls, we merge contiguous chunks
		// together. Since the concept of a single 'allocation' does not exist in the physical allocator, this is
		// allowed regardless of whether the chunks were allocated in one go.
		// If the memory is contiguous, we deallocate it in one go by converting it to an OwnedFrames object and
		// immediately dropping it.
		match self.contiguity {
			RawMappingContiguity::Contiguous(base_frame) => {
				let _frames = unsafe { OwnedFrames::from_raw_parts(
					base_frame,
					self.physical_len(),
					self.allocator,
				) };
			},
			RawMappingContiguity::Discontiguous => {
				let page_iter = (0..self.physical_len().get()).map(|i| self.virtual_valid_start() + i);
				let frame_iter = page_iter.map(|page| unsafe {
					crate::bridge::paging::__popcorn_paging_ktable_translate_page(&mut page_table, page)
							.expect("Virtual memory uniquely owned by this mmap so shouldn't be unmapped")
				});

				let drop_frame_range = |(frame, len)| {
					let _frames = unsafe { OwnedFrames::from_raw_parts(
						frame,
						len,
						self.allocator,
					) };
				};

				// Group the frames into contiguous chunks
				// First convert into a tuple of `(start, len)`, where each is of len 1
				// Then reduce: if the end of one frame range is the start of the next,
				// combine into a range with the new length; otherwise, take the already
				// combined chunks, and deallocate in one go
				let last_chunk = frame_iter.map(|frame| (frame, NonZero::<usize>::new(1).unwrap()))
						.reduce(|prev_group, new_group| {
							if prev_group.0 + prev_group.1.get() == new_group.0
								&& let Some(combined_len) = prev_group.1.checked_add(new_group.1.get()) { // If the length overflows, just drop in two chunks
								// contiguous, so merge
								(prev_group.0, combined_len)
							} else {
								// discontigous, so drop the existing set
								drop_frame_range(prev_group);
								new_group
							}
						});
				if let Some(last_chunk) = last_chunk {
					drop_frame_range(last_chunk);
				}
			},
		}

		for page in (0..self.physical_len().get()).map(|i| self.virtual_valid_start() + i) {
			debug!("unmapping page {page:x?}");
			unsafe { crate::bridge::paging::__popcorn_paging_ktable_unmap_page(&mut page_table, page) }
					.expect("Virtual memory uniquely owned by this mmap so shouldn't be unmapped");
		}

		let virtual_allocator = unsafe { ManuallyDrop::take(&mut self.virtual_allocator) };
		let _pages = unsafe {
			OwnedPages::from_raw_parts(
				self.virtual_start(),
				self.virtual_len(),
				virtual_allocator
			)
		};
	}
}

#[doc(hidden)]
#[stable(feature = "kernel_mmap", since = "1.1.0")]
pub enum RawMmap {}

#[stable(feature = "kernel_mmap", since = "1.1.0")]
impl Mappable for RawMmap {
	fn physical_length_to_virtual_length(physical_length: NonZero<usize>) -> NonZero<usize> { physical_length }
	fn physical_start_offset_from_virtual() -> isize { 0 }
}

#[doc(hidden)]
#[stable(feature = "kernel_mmap", since = "1.1.0")]
pub enum RawStack {}

#[stable(feature = "kernel_mmap", since = "1.1.0")]
impl Mappable for RawStack {
	fn physical_length_to_virtual_length(physical_length: NonZero<usize>) -> NonZero<usize> {
		physical_length.checked_add(1).expect("Stack size overflow")
	}
	fn physical_start_offset_from_virtual() -> isize { 1 }
}

/// A RAII memory mapping
///
/// Manages a mapping directly between physical and virtual memory.
#[allow(type_alias_bounds)] // makes docs nicer
#[stable(feature = "kernel_mmap", since = "1.1.0")]
pub type Mapping<'phys_alloc, V: VirtualAllocator = Global> = RawMapping<'phys_alloc, RawMmap, V>;

/// A RAII stack
///
/// Manages the memory map for a stack, including a guard page below the stack.
#[allow(type_alias_bounds)] // makes docs nicer
#[stable(feature = "kernel_mmap", since = "1.1.0")]
pub type Stack<'phys_alloc, V: VirtualAllocator = Global> = RawMapping<'phys_alloc, RawStack, V>;
