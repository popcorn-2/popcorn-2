#![no_std]
#![feature(never_type)]

extern crate alloc;

use core::fmt;
use core::marker::PhantomData;
use core::mem::{size_of};
use core::ops::{Index, IndexMut, Range};
use core::ptr::{slice_from_raw_parts, slice_from_raw_parts_mut};
use header::file::FileHeader;
use header::program::ProgramHeaderEntry64;

mod utils;
pub mod symbol_table;
pub mod string_table;
pub mod relocation;
pub mod dynamic_table;
pub mod header;

type Error = header::file::Error;

macro_rules! file {
	(@ref mut $ty:ty) => { &'a mut $ty };
	(@ref const $ty:ty) => { &'a $ty };
	(@slice mut $($arg:tt)*) => { slice_from_raw_parts_mut($($arg)*) };
	(@slice const $($arg:tt)*) => { slice_from_raw_parts($($arg)*) };
    ($ty:ident $m:tt) => {
	    #[derive(Debug)]
		#[repr(C)]
		pub struct $ty<'a> {
			/// Base address of the executable
			/// Can be adjusted by calling [`File::relocate()`]
			base: u64,
			header: &'a FileHeader,
			program_header: &'a [ProgramHeaderEntry64],
			section_header: &'a [u8],
			/// The contents of the executable (including all headers)
			data: * $m [u8],
			_phantom: PhantomData<file!(@ref $m [u8])>
		}
	    
	    impl<'a> $ty<'a> {
			pub fn try_new(elf_data: file!(@ref $m [u8])) -> Result< $ty <'a>, Error> {
				let data_len = elf_data.len();
				let data_ptr = elf_data.as_ptr();
				let header = &elf_data[..size_of::<FileHeader>()];
		
				FileHeader::try_new(header).map(|header| {
					let program_header = {
						let Range{ start, end } = header.program_header();
						let count = (end - start) / header.program_header_entry_size();
						unsafe {
							// SAFETY:
							// start is non-null since taken from a non-null data pointer
							// alignment checked by assertion
							// reference only aliases with immutable reference to data
							let start = data_ptr.byte_add(start).cast::<ProgramHeaderEntry64>();
							assert!(start.is_aligned());
						    &*slice_from_raw_parts(start, count)
						}
					};
		
					let section_header = {
						let Range{ start, end } = header.section_header();
						let count = (end - start) / header.section_header_entry_size();
						unsafe {
							// SAFETY:
							// start is non-null since taken from a non-null data pointer
							// alignment checked by assertion
							// reference only aliases with immutable reference to data
							let start = data_ptr.byte_add(start).cast::<u8>();
							assert!(start.is_aligned());
							&*slice_from_raw_parts(start, count)
						}
					};
		
					$ty {
						base: 0,
						header,
						program_header,
						section_header,
						data: file!(@slice $m data_ptr as * $m _, data_len),
						_phantom: PhantomData
					}
				})
			}
		
			pub fn segments(&self) -> impl Iterator<Item = ProgramHeaderEntry64> + '_ {
				self.program_header.iter()
						.map(|entry| {
							ProgramHeaderEntry64 {
								vaddr: entry.vaddr + self.base,
								.. *entry
							}
						})
			}
		
			fn index_data(&self, slice: FileLocation) -> &[u8] {
				// SAFETY: self.data must be valid, and returning an immutable reference so fine to alias with self.{program,section}_header
				unsafe {
					&(*self.data)[slice.0]
				}
			}
		    
		    fn segment_for_address(&self, addr: ExecutableAddressRelocated) -> Option<ProgramHeaderEntry64> {
				self.segments().find(|segment| segment.memory_location().contains(&addr.0))
			}
		
			pub fn data_at_address(&self, addr: ExecutableAddressRelocated) -> Option<*const u8> {
				let segment = self.segment_for_address(addr)?;
				Some(unsafe {
					self[segment.file_location()].as_ptr().byte_add(usize::try_from(addr.0 - segment.vaddr).unwrap())
				})
			}
		    
		    fn data_at_unrel_address(&self, addr: ExecutableAddressUnrelocated) -> Option<*const u8> { self.data_at_address(ExecutableAddressRelocated(addr.0 + self.base)) }
			
			pub fn entrypoint(&self) -> usize {
				self.header.entry_point()
			}
		}
	    
	    impl<'a> Index<FileLocation> for $ty <'a> {
			type Output = [u8];
		
			fn index(&self, index: FileLocation) -> &Self::Output {
				self.index_data(index)
			}
		}
    };
}

file!(File const);
file!(FileMut mut);

impl<'a> FileMut<'a> {
		fn index_data_mut(&mut self, slice: FileLocation) -> &mut [u8] {
		fn ranges_overlap(a: Range<usize>, b: Range<usize>) -> bool {
			!(a.start >= b.end || b.start >= a.end)
		}

		assert!(
			!ranges_overlap(slice.0.clone(), 0..size_of::<FileHeader>()) &&
			!ranges_overlap(slice.0.clone(), self.header.program_header()) &&
			!ranges_overlap(slice.0.clone(), self.header.section_header()),
			"Cannot mutably index into headers"
		);

		// SAFETY: self.data must be valid, and checked that reference won't alias
		unsafe {
			&mut (*self.data)[slice.0]
		}
	}

	pub fn data_at_address_mut(&mut self, addr: ExecutableAddressRelocated) -> Option<*mut u8> {
		let segment = self.segment_for_address(addr)?;
		Some(unsafe {
			self[segment.file_location()].as_mut_ptr().byte_add(usize::try_from(addr.0 - segment.vaddr).unwrap())
		})
	}

	fn data_at_unrel_address_mut(&mut self, addr: ExecutableAddressUnrelocated) -> Option<*mut u8> { self.data_at_address_mut(ExecutableAddressRelocated(addr.0 + self.base)) }
}

impl<'a> IndexMut<FileLocation> for FileMut<'a> {
	fn index_mut(&mut self, index: FileLocation) -> &mut Self::Output {
		self.index_data_mut(index)
	}
}

#[derive(Debug, Clone)]
#[repr(transparent)]
pub struct FileLocation(Range<usize>);

macro_rules! derive_fmt_filelocation {
    ($($fmt: path)*) => {
		$(impl $fmt for FileLocation {
			fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
				f.write_str("(")?;
				<usize as $fmt>::fmt(&self.0.start, f)?;
				f.write_str(", ")?;
				<usize as $fmt>::fmt(&self.0.end, f)?;
				f.write_str(")")?;
				Ok(())
			}
		})*
	};
}

derive_fmt_filelocation!(fmt::Display fmt::Binary fmt::LowerHex fmt::UpperHex fmt::Octal);

#[derive(Debug, Copy, Clone)]
#[repr(transparent)]
pub struct ExecutableAddressRelocated(u64);

impl ExecutableAddressRelocated {
	pub fn get(self) -> u64 {
		self.0
	}
}

#[derive(Debug, Copy, Clone)]
#[repr(transparent)]
pub struct ExecutableAddressUnrelocated(u64);

impl ExecutableAddressUnrelocated {
	unsafe fn relocate(self, base: u64) -> ExecutableAddressRelocated {
		ExecutableAddressRelocated(self.0 + base)
	}
}
