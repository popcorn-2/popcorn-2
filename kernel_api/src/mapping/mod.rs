//! RAII memory mappings

mod config;
pub use config::*;

#[cfg(feature = "full")] mod mappable;
#[cfg(feature = "full")] pub use mappable::*;
#[cfg(feature = "full")] pub use full::*;

#[cfg(feature = "full")]
mod full {
	use super::*;
	use core::fmt::{Debug, Formatter};
	use crate::{address_space, dbg};
	use core::mem::ManuallyDrop;
	use core::num::NonZero;
	use core::ops::Range;
	use core::{mem, ptr};
	use log::{debug, warn};
	use crate::allocator::{AllocError, DynPmm};
	use crate::memory::{Frames, RawFrame, RawPage, PAGE_SIZE};
	use crate::ptr::User;

	#[derive(Debug)]
	pub enum MapPageError {
		AlreadyMapped(Ty),
		AllocError,
	}

	impl From<AllocError> for MapPageError {
		fn from(_value: AllocError) -> Self {
			Self::AllocError
		}
	}

	/// Used to track if the memory underlying the mapping is contiguous
	pub(super) enum Backing {
		/// The underlying physical memory is contiguous, and starts at the contained frame
		Contiguous(Frames<false>),

		/// The underlying physical memory is discontiguous, but all allocated by the same
		Discontiguous { pmm: DynPmm<'static, false>, frame_count: usize },
	}

	impl Backing {
		fn frame_len(&self) -> usize {
			match self {
				Backing::Contiguous(frames) => frames.count(),
				Backing::Discontiguous { frame_count, .. } => *frame_count,
			}
		}

		fn byte_len(&self) -> usize {
			self.frame_len() * PAGE_SIZE
		}

		fn pmm(&self) -> &DynPmm<'static, false> {
			match self {
				Backing::Contiguous(frames) => frames.pmm(),
				Backing::Discontiguous { pmm, .. } => pmm,
			}
		}
	}

	/// The raw type underlying all memory mappings.
	///
	/// This will allocate any required memory when created, and register any lazily mapped memory as such.
	/// It will also manage the page tables to correctly unmap the memory when dropped.
	pub struct Mapping<R, A> {
		pub(super) raw: R,

		/// The address space mapped into
		pub(super) address_space: ManuallyDrop<A>,

		pub(super) backing: Backing,
		//#[cfg(feature = "use_std")] pub(super) backing: *mut libc::c_void,

		pub(super) caching: Caching,

		pub(super) virtual_start: RawPage,

		/// The protection used when mapping pages into this mapping
		pub(super) protection: Protection,
	}

	impl<R: Mappable> Mapping<R, address_space::Kernel> {
		pub fn as_mut_ptr_range(&mut self) -> Range<*mut u8> {
			let start = self.virtual_valid_start().as_ptr();
			Range {
				start,
				end: unsafe { start.byte_add(self.byte_len()) }
			}
		}

		pub fn as_ptr_range(&self) -> Range<*const u8> {
			let start = self.virtual_valid_start().as_ptr().cast_const();
			Range {
				start,
				end: unsafe { start.byte_add(self.byte_len()) }
			}
		}

		pub fn as_mut_ptr(&mut self) -> *mut u8 {
			self.as_mut_ptr_range().start
		}

		pub fn as_ptr(&self) -> *const u8 {
			self.as_ptr_range().start
		}

		//#[cfg(not(feature = "use_std"))]
		pub unsafe fn from_raw_parts<const RAM: bool, T>(
			frames: Frames<RAM, T>,
			base_page: RawPage,
			protection: Protection,
			caching: Caching,
		) -> Self where R: Default {
			Self {
				raw: R::default(),
				address_space: ManuallyDrop::new(address_space::Kernel {}),
				backing: Backing::Contiguous(unsafe { Frames::<false>::from_raw_tuple(Frames::into_raw(frames)) }),
				virtual_start: base_page,
				protection,
				caching,
			}
		}

		//#[cfg(not(feature = "use_std"))]
		pub fn into_raw_parts(self) -> (Option<Frames<false>>, RawPage, Protection, Caching) {
			let this = ManuallyDrop::new(self);
			(
				match unsafe { ptr::read(&this.backing) } {
					Backing::Contiguous(frames) => Some(frames),
					Backing::Discontiguous { .. } => None,
				},
				this.virtual_start,
				this.protection,
				this.caching,
			)
		}
	}

	#[cfg(not(feature = "use_std"))]
	impl<R: Mappable> Mapping<R, address_space::Userspace> {
		pub fn as_mut_ptr_range(&mut self) -> Range<User<'_, *mut u8>> {
			let start = self.virtual_valid_start().as_ptr();
			let end = unsafe { start.byte_add(self.byte_len()) };
			let start = User::<*mut u8>::new(start, &self.address_space.inner);
			let end = User::<*mut u8>::new(end, &self.address_space.inner);
			start..end
		}

		pub fn as_ptr_range(&self) -> Range<User<'_, *const u8>> {
			let start = self.virtual_valid_start().as_ptr().cast_const();
			let end = unsafe { start.byte_add(self.byte_len()) };
			let start = unsafe { User::<*const u8>::new(start, &self.address_space.inner) };
			let end = unsafe { User::<*const u8>::new(end, &self.address_space.inner) };
			start..end
		}

		pub fn as_mut_ptr(&mut self) -> User<'_, *mut u8> {
			self.as_mut_ptr_range().start
		}

		pub fn as_ptr(&self) -> User<'_, *const u8> {
			self.as_ptr_range().start
		}
	}

	impl<R: Mappable, A: address_space::Ty> Mapping<R, A> {
		pub fn grow_in_place_by(&mut self, extra_length: usize) -> Result<(), AllocError> {
			let Some(extra_length) = NonZero::new(extra_length) else { return Ok(()); };
			let extra_frames = self.backing.pmm().allocate(extra_length)?;

			let extra_pages = self.address_space.allocator()
			                      .allocate_contiguous_at(
				                      self.virtual_start + self.raw.virtual_size(self.page_len()).get(),
				                      extra_length.get(),
			                      )?;

			debug!("growing mmap({})", core::any::type_name::<R>());
			let _ = dbg!(self.virtual_start);
			let _ = dbg!(self.virtual_valid_start());
			let _ = dbg!(self.page_len());
			let _ = dbg!(extra_length);
			let _ = dbg!(self.raw.virtual_size(self.page_len()));

			match self.address_space.map_contiguous(
				self.virtual_valid_start() + self.page_len(),
				extra_frames.into_raw().0.start,
				extra_length.get(),
				Ty(0),
				self.protection,
				self.caching,
			) {
				Ok(_) => Ok(()),
				Err(MapPageError::AllocError) => {
					self.address_space.allocator()
							.deallocate_contiguous(extra_pages, extra_length.get());
					Err(AllocError::default())
				}
				Err(MapPageError::AlreadyMapped(ty)) => unreachable!("unallocated memory already allocated as {ty:?}"),
			}?;

			let new_backing = Backing::Discontiguous {
				pmm: *self.backing.pmm(),
				frame_count: self.backing.frame_len().checked_add(extra_length.get()).unwrap()
			};
			mem::forget(mem::replace(&mut self.backing, new_backing));

			Ok(())
		}

		pub fn byte_len(&self) -> usize {
			self.backing.byte_len()
		}

		pub fn page_len(&self) -> usize {
			self.backing.frame_len()
		}

		pub fn physical_start(&self) -> Option<RawFrame> {
			match &self.backing {
				Backing::Contiguous(frames) => Some(frames.base()),
				Backing::Discontiguous { .. } => None,
			}
		}

		/// The first page in the mapping that is mapped to physical memory.
		/// The region of virtual memory from `virtual_valid_start()` to `virtual_valid_start() + physical_length` is mapped.
		pub fn virtual_valid_start(&self) -> RawPage { self.virtual_start + self.raw.base_virtual_offset() }

		pub fn set_writable(&mut self, writable: bool) {
			self.protection.writable = writable;
			todo!()
		}
	}

	impl<R: Mappable, A: address_space::Ty> Debug for Mapping<R, A> {
		fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
			f.debug_struct("Mapping")
			 .field_with(
				 "backing",
				 |f| {
					 #[cfg(not(feature = "use_std"))] match self.backing {
						 Backing::Contiguous(ref frame) => Debug::fmt(frame, f),
						 Backing::Discontiguous { frame_count, .. } => write!(f, "Discontiguous {{ frame_count: {} }}", frame_count),
					 }
					 #[cfg(feature = "use_std")] Ok(())
				 }
			 )
			 .field("address_space", &"<address space>")
			 .field("protection", &self.protection)
			 .field("caching", &self.caching)
			 .finish_non_exhaustive()
		}
	}

	impl<R, A> Drop for Mapping<R, A> {
		fn drop(&mut self) {
			warn!("Ignoring drop of mmap({})", core::any::type_name::<R>());
		}
	}
}
