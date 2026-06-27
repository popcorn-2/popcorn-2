//! Memory mappings.
//!
//! Each memory mapping is made of a virtual allocation and a physical allocation,
//! where the size of the virtual allocation is equal to or larger than the size
//! of the physical allocation.
//!
//! The physical memory is then mapped to a space within the virtual allocation,
//! and the rest of the virtual allocation is left reserved but inaccessible.
//! The size of the physical allocation (and thus usable memory) is the value
//! requested by users of the mapping API.
//!
//! The size of and offset within the virtual allocation is controlled by implementations
//! of [`Mappable`], as shown below.
//!
//! ```text
//! ---+------------------------------+---
//!    |      virtual allocation      |
//! ---+------------------------------+---
//!     <----------------------------> `virtual_size()`
//!     <---> `base_virtual_offset()`
//!          +---------------------+
//!          | physical allocation |
//!          +---------------------+
//! ```
//!
//! A [`Mapping`] can be created with [`Config::map`]:
//!
//! ```
//! use kernel_api::mapping::{Config, Ty, Mmap};
//! use core::num::NonZero;
//!
//! let mapping = Config::new(NonZero::new(1).unwrap(), Ty::KERNEL_OTHER)
//!                   .map::<Mmap>()?;
//! # Ok::<(), kernel_api::allocator::AllocError>::(())
//! ```
//!
//! Data can then be read from and written to the mapping:
//!
//! ```
//! use kernel_api::mapping::{Config, Ty, Mmap};
//! use core::num::NonZero;
//!
//! let mut mapping = Config::new(NonZero::new(1).unwrap(), Ty::KERNEL_OTHER)
//!                       .protection(/* writable: */ true, false, false)
//!                       .map::<Mmap>()?;
//!
//! for ptr in mapping.as_mut_ptr_range() {
//!     unsafe { *ptr = 1 };
//! }
//!
//! for ptr in mapping.as_ptr_range() {
//!     assert_eq!(unsafe { *ptr }, 1);
//! }
//! # Ok::<(), kernel_api::allocator::AllocError>::(())
//! ```

#[cfg(feature = "full")] mod config;
#[cfg(feature = "full")] pub use config::*;

#[cfg(feature = "full")] mod mappable;
#[cfg(feature = "full")] pub use mappable::*;

#[cfg(feature = "full")] include!("real_mod.rs");

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
