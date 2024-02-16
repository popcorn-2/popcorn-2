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

#![unstable(feature = "kernel_mmap", issue = "24")]

use core::fmt::{Debug, Formatter};
use core::marker::PhantomData;
use core::mem::ManuallyDrop;
use core::num::{NonZeroU32, NonZeroUsize};
use core::ptr;
use log::debug;
use crate::memory::allocator::{AlignedAllocError, AllocationMeta, BackingAllocator, ZeroAllocError};
use crate::memory::{AllocError, Frame, Page};
use crate::memory::physical::{OwnedFrames, highmem};
use crate::memory::r#virtual::{Global, OwnedPages, VirtualAllocator};

#[deprecated(since = "0.1.0", note = "Use Mapping instead")]
#[unstable(feature = "kernel_mmap", issue = "24")]
#[derive(Copy, Clone, Debug)]
pub struct Highmem;

unsafe impl BackingAllocator for Highmem {
	fn allocate_contiguous(&self, frame_count: usize) -> Result<Frame, AllocError> {
		highmem().allocate_contiguous(frame_count)
	}

	fn allocate_one(&self) -> Result<Frame, AllocError> {
		highmem().allocate_one()
	}

	fn try_allocate_zeroed(&self, frame_count: usize) -> Result<Frame, ZeroAllocError> {
		highmem().try_allocate_zeroed(frame_count)
	}

	fn allocate_zeroed(&self, frame_count: usize) -> Result<Frame, AllocError> {
		highmem().allocate_zeroed(frame_count)
	}

	unsafe fn deallocate_contiguous(&self, base: Frame, frame_count: NonZeroUsize) {
		highmem().deallocate_contiguous(base, frame_count)
	}

	fn push(&mut self, allocation: AllocationMeta) {
		unimplemented!()
	}

		highmem().allocate_contiguous_aligned(count, alignment_log2)
	fn allocate_contiguous_aligned(&self, count: NonZeroUsize, alignment_log2: u32) -> Result<Frame, AllocError> {
	}

	fn try_allocate_contiguous_aligned(&self, count: NonZeroUsize, alignment_log2: u32) -> Result<Frame, AlignedAllocError> {
		highmem().try_allocate_contiguous_aligned(count, alignment_log2)
	}
}

/// An owned region of memory
///
/// Depending on memory attributes, this may be invalid to read or write to
#[deprecated(since = "0.1.0", note = "Use Mapping instead")]
#[unstable(feature = "kernel_mmap", issue = "24")]
#[derive(Debug)]
pub struct OldMapping<A: BackingAllocator = Highmem> {
	base: Page,
	len: usize,
	allocator: A
}

impl<A: BackingAllocator> OldMapping<A> {
	pub fn as_ptr(&self) -> *const u8 {
		self.base.as_ptr()
	}
	
	pub fn as_mut_ptr(&mut self) -> *mut u8 {
		self.base.as_ptr()
	}
	
	pub fn new_with(len: usize, physical_allocator: A) -> Result<Self, AllocError> {
		// FIXME: memory leak here on error from lack of ArcFrame
		let physical_mem = physical_allocator.allocate_contiguous(len)?;
		let virtual_mem = Global.allocate_contiguous(len)?;

		// TODO: huge pages
		let mut page_table = unsafe { crate::bridge::paging::__popcorn_paging_get_ktable() };
		for (frame, page) in (0..len).map(|i| (physical_mem + i, virtual_mem + i)) {
			unsafe { crate::bridge::paging::__popcorn_paging_ktable_map_page(&mut page_table, page, frame) }
					.expect("todo");
		}

		Ok(Self {
			base: virtual_mem,
			len,
			allocator: physical_allocator
		})
	}
}

impl OldMapping<Highmem> {
	pub fn new(len: usize) -> Result<Self, AllocError> {
		Self::new_with(len, Highmem)
	}

	fn resize_inner(&mut self, new_len: usize) -> Result<(), Option<Frame>> {
		if new_len == self.len { return Ok(()); }

		// FIXME: DOnT JUST USE HIGHMEM UnCOnDITIOnALLY
		let original_physical_allocator = self.allocator;

		if new_len < self.len {
			// todo: actually free and unmap the extra memory

			self.len = new_len;
			Ok(())
		} else {
			let extra_len = new_len - self.len;

			debug!("allocating extra physical memory");
			// fixme: physical mem leak
			let extra_physical_mem = original_physical_allocator.allocate_contiguous(extra_len).map_err(|_| None)?;
			debug!("allocating extra virtual memory");
			let extra_virtual_mem = Global.allocate_contiguous_at(self.base + self.len, extra_len);

			match extra_virtual_mem {
				Ok(_) => {
					let start_of_extra = self.base + self.len;

					// TODO: huge pages
					let mut page_table = unsafe { crate::bridge::paging::__popcorn_paging_get_ktable() };

					for (frame, page) in (0..extra_len).map(|i| (extra_physical_mem + i, start_of_extra + i)) {
						unsafe { crate::bridge::paging::__popcorn_paging_ktable_map_page(&mut page_table, page, frame) }
								.expect("todo");
					}

					self.len = new_len;
					Ok(())
				}
				Err(_) => Err(Some(extra_physical_mem))
			}
		}
	}

	pub fn resize_in_place(&mut self, new_len: usize) -> Result<(), AllocError> {
		self.resize_inner(new_len)
				.map_err(|_| AllocError)
	}

	pub fn resize(&mut self, new_len: usize) -> Result<(), AllocError> {
		match self.resize_inner(new_len) {
			Ok(_) => Ok(()),
			Err(None) => Err(AllocError),
			Err(Some(extra_physical_mem)) => {
				// can assume here that new_len > len as shrinking can't fail

				// FIXME: DOnT JUST USE HIGHMEM UnCOnDITIOnALLY
				let original_physical_allocator = self.allocator;

				let extra_len = new_len - self.len;
				let new_virtual_mem = Global.allocate_contiguous(new_len)?;

				let mut page_table = unsafe { crate::bridge::paging::__popcorn_paging_get_ktable() };

				let physical_base: Frame = todo!();
				for (frame, page) in (0..self.len).map(|i| (physical_base + i, new_virtual_mem + i)) {
					unsafe { crate::bridge::paging::__popcorn_paging_ktable_map_page(&mut page_table, page, frame) }.expect("todo");
				}
				for (frame, page) in (0..extra_len).map(|i| (extra_physical_mem + i, new_virtual_mem + self.len + i)) {
					unsafe { crate::bridge::paging::__popcorn_paging_ktable_map_page(&mut page_table, page, frame) }.expect("todo");
				}

				self.base = new_virtual_mem;
				self.len = new_len;

				Ok(())
			}
		}
	}

	/*pub fn remap(&mut self, new_len: usize) -> Result<(), AllocError> {
		if new_len == self.len { return Ok(()); }

		let original_physical_allocator: &dyn BackingAllocator = todo!("retrieve original allocator");
		let physical_base: Frame = todo!("translate base to locate physical backing");

		if new_len < self.len {
			// todo: actually free and unmap the extra memory

			self.len = new_len;
			Ok(())
		} else {
			let extra_len = new_len - self.len;

			let extra_physical_mem = original_physical_allocator.allocate_contiguous(extra_len)?;
			let extra_virtual_mem = Global.allocate_contiguous_at(self.base + self.len, extra_len);

			match extra_virtual_mem {
				Ok(_) => {
					let start_of_extra = self.base + self.len;

					// TODO: huge pages
					let mut page_table = current_page_table();

					for (frame, page) in (0..extra_len).map(|i| (extra_physical_mem + i, start_of_extra + i)) {
						page_table.map_page(page, frame, original_physical_allocator).expect("todo");
					}

					self.len = new_len;
					Ok(())
				}
				Err(_) => {
					let new_virtual_mem = Global.allocate_contiguous(new_len)?;

					let mut page_table = current_page_table();

					for (frame, page) in (0..self.len).map(|i| (physical_base + i, new_virtual_mem + i)) {
						page_table.map_page(page, frame, original_physical_allocator).expect("todo");
					}
					for (frame, page) in (0..extra_len).map(|i| (extra_physical_mem + i, new_virtual_mem + self.len + i)) {
						page_table.map_page(page, frame, original_physical_allocator).expect("todo");
					}

					self.base = new_virtual_mem;
					self.len = new_len;

					Ok(())
				}
			}
		}
	}*/

	pub fn into_raw_parts(self) -> (Page, usize) {
		let Self { base, len, .. } = self;
		(base, len)
	}

	pub unsafe fn from_raw_parts(base: Page, len: usize) -> Self {
		Self {
			base,
			len,
			allocator: Highmem
		}
	}

	pub fn end(&self) -> Page {
		self.base + self.len
	}

	pub fn len(&self) -> usize {
		self.len
	}
}

impl<A: BackingAllocator> Drop for OldMapping<A> {
	fn drop(&mut self) {
		// todo
	}
}

/// Basic operations to decide how to map memory together.
///
/// Implementations of this can be used to instantiate a [`RawMapping`].
pub trait Mappable {
	/// The amount of virtual memory required to create a mapping with `physical_length` [`Frame`]s
	fn physical_length_to_virtual_length(physical_length: NonZeroUsize) -> NonZeroUsize;

	/// The number of [`Page`]s to offset the physical memory into the allocated virtual memory
	fn physical_start_offset_from_virtual() -> isize;
}

/// The memory protection to use for the memory mapping
pub enum Protection {
	/// The mapping is read-write and can be executed from
	RWX
}

mod private {
	use crate::memory::{Frame, Page};

	pub trait Sealed {}

	impl Sealed for Page {}
	impl Sealed for Frame {}
}

/// A marker trait for types that can be used as a [`Location`]
pub trait Address: private::Sealed {}
impl Address for Page {}
impl Address for Frame {}

/// The location at which to make the [mapping](self)
pub enum Location<A: Address> {
	/// The mapping can go anywhere
	Any,
	/// The mapping must be aligned to a specific number of [`Page`]s/[`Frame`]s
	Aligned(NonZeroU32),
	/// The mapping will fail if it cannot be allocated at this exact location
	At(A),
	/// The mapping must be below this location, aligned to `with_alignment` number of [`Page`]s/[`Frame`]s
	Below { location: A, with_alignment: NonZeroU32 }
}

/// When to allocate physical memory for the [mapping](self)
pub enum Laziness { Lazy, Prefault }

/// Configuration for creating a [mapping](self)
///
/// By default, it will allocate memory anywhere that is valid, using the kernel [`VirtualAllocator`], and the
/// `highmem` [`physical allocator`](BackingAllocator). It will lazily allocate physical memory, and map it
/// with read and write permissions only.
pub struct Config<'physical_allocator, A: VirtualAllocator> {
	physical_location: Location<Frame>,
	virtual_location: Location<Page>,
	laziness: Laziness,
	length: NonZeroUsize,
	physical_allocator: &'physical_allocator dyn BackingAllocator,
	virtual_allocator: A,
	protection: Protection,
}

impl<'physical_allocator, A: VirtualAllocator> Config<'physical_allocator, A> {
	/// Creates a new [mapping](self) configuration with default options
	pub fn new(length: NonZeroUsize) -> Config<'static, Global> {
		Config {
			physical_location: Location::Any,
			virtual_location: Location::Any,
			laziness: Laziness::Lazy,
			length,
			physical_allocator: highmem(),
			virtual_allocator: Global,
			protection: Protection::RWX,
		}
	}

	pub fn physical_allocator<'a>(self, allocator: &'a dyn BackingAllocator) -> Config<'a, A> {
		Config {
			physical_allocator: allocator,
			.. self
		}
	}

	pub fn virtual_allocator<New: VirtualAllocator>(self, allocator: New) -> Config<'physical_allocator, New> {
		Config {
			virtual_allocator: allocator,
			.. self
		}
	}

	pub fn protection(self, protection: Protection) -> Self {
		Config {
			protection,
			.. self
		}
	}

	pub fn physical_location(self, location: Location<Frame>) -> Self {
		Config {
			physical_location: location,
			.. self
		}
	}

	pub fn virtual_location(self, location: Location<Page>) -> Self {
		Config {
			virtual_location: location,
			.. self
		}
	}
}

/// The raw type underlying all memory mappings.
///
/// This will allocate any required memory when created, and register any lazily mapped memory as such.
/// It will also manage the page tables to correctly unmap the memory when dropped.
pub struct RawMapping<'phys_allocator, R: Mappable, A: VirtualAllocator> {
	raw: PhantomData<R>,
	physical: OwnedFrames<'phys_allocator>,
	virtual_base: Page,
	virtual_allocator: ManuallyDrop<A>,
}

impl<R: Mappable, A: VirtualAllocator> Debug for RawMapping<'_, R, A> {
	fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
		f.debug_struct("RawMapping")
		 .field("physical", &self.physical)
		 .field("virtual_base", &self.virtual_base)
		 .field("virtual_allocator", &"<virtual allocator>")
		 .finish()
	}
}

impl<'phys_alloc, R: Mappable, A: VirtualAllocator> RawMapping<'phys_alloc, R, A> {
	pub fn new(config: Config<'phys_alloc, A>) -> Result<Self, AllocError> {
		let Config { length, physical_allocator, virtual_allocator, .. } = config;

		let virtual_len = R::physical_length_to_virtual_length(length);
		let physical_len = length;

		let physical_mem = OwnedFrames::new_with(physical_len, physical_allocator)?;
		let virtual_mem = OwnedPages::new_with(virtual_len, virtual_allocator)?;

		let physical_base = physical_mem.base;
		let (virtual_base, _, virtual_allocator) = virtual_mem.into_raw_parts();
		let offset_base = virtual_base + R::physical_start_offset_from_virtual();

		// TODO: huge pages
		let mut page_table = unsafe { crate::bridge::paging::__popcorn_paging_get_ktable() };
		for (frame, page) in (0..physical_len.get()).map(|i| (physical_base + i, offset_base + i)) {
			unsafe { crate::bridge::paging::__popcorn_paging_ktable_map_page(&mut page_table, page, frame) }
					.expect("Virtual memory uniquely owned by the allocation so should not be mapped in this address space");
		}

		Ok(Self {
			raw: PhantomData,
			physical: physical_mem,
			virtual_base,
			virtual_allocator: ManuallyDrop::new(virtual_allocator)
		})
	}

	pub fn into_raw_parts(mut self) -> (OwnedFrames<'phys_alloc>, OwnedPages<A>) {
		let virtual_allocator = unsafe { ManuallyDrop::take(&mut self.virtual_allocator) };
		let pages = unsafe {
			OwnedPages::from_raw_parts(
				self.virtual_base,
				R::physical_length_to_virtual_length(self.physical.len),
				virtual_allocator
			)
		};

		let this = ManuallyDrop::new(self);
		(unsafe { ptr::read(&this.physical) }, pages)
	}

	pub unsafe fn from_raw_parts(frames: OwnedFrames<'phys_alloc>, pages: OwnedPages<A>) -> Self {
		let (virtual_base, actual_vlen, virtual_allocator) = pages.into_raw_parts();
		let correct_vlen = R::physical_length_to_virtual_length(frames.len);
		debug_assert_eq!(actual_vlen, correct_vlen);

		Self {
			raw: PhantomData,
			physical: frames,
			virtual_base,
			virtual_allocator: ManuallyDrop::new(virtual_allocator)
		}
	}

	fn virtual_len(&self) -> NonZeroUsize {
		R::physical_length_to_virtual_length(self.physical.len)
	}

	pub fn virtual_start(&self) -> Page {
		self.virtual_base
	}

	pub fn virtual_end(&self) -> Page {
		self.virtual_start() + self.virtual_len().get()
	}

	pub fn physical_len(&self) -> NonZeroUsize {
		self.physical.len
	}

	pub fn physical_start(&self) -> Frame {
		self.physical.base
	}

	pub fn physical_end(&self) -> Frame {
		self.physical_start() + self.physical_len().get()
	}
}

impl<R: Mappable, A: VirtualAllocator> Drop for RawMapping<'_, R, A> {
	fn drop(&mut self) {
		// todo: unmap stuff

		let virtual_allocator = unsafe { ManuallyDrop::take(&mut self.virtual_allocator) };
		let _pages = unsafe {
			OwnedPages::from_raw_parts(
				self.virtual_base,
				R::physical_length_to_virtual_length(self.physical.len),
				virtual_allocator
			)
		};
	}
}

#[doc(hidden)]
pub enum RawMmap {}

impl Mappable for RawMmap {
	fn physical_length_to_virtual_length(physical_length: NonZeroUsize) -> NonZeroUsize { physical_length }
	fn physical_start_offset_from_virtual() -> isize { 0 }
}

#[doc(hidden)]
pub enum RawStack {}

impl Mappable for RawStack {
	fn physical_length_to_virtual_length(physical_length: NonZeroUsize) -> NonZeroUsize {
		physical_length.checked_add(1).expect("Stack size overflow")
	}
	fn physical_start_offset_from_virtual() -> isize { 1 }
}

/// A RAII memory mapping
///
/// Manages a mapping directly between physical and virtual memory.
#[allow(type_alias_bounds)] // makes docs nicer
pub type Mapping<'phys_alloc, V: VirtualAllocator = Global> = RawMapping<'phys_alloc, RawMmap, V>;

/// A RAII stack
///
/// Manages the memory map for a stack, including a guard page below the stack.
#[allow(type_alias_bounds)] // makes docs nicer
pub type Stack<'phys_alloc, V: VirtualAllocator = Global> = RawMapping<'phys_alloc, RawStack, V>;
