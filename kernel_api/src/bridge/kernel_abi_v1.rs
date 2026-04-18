pub mod irq {
	unsafe extern "Rust" {
		#[link_name = "__popcorn_set_irq"]
		pub safe fn set(state: usize);
		
		#[link_name = "__popcorn_disable_irq"]
		pub safe fn disable() -> usize;
	}
}

pub mod memory {
	use crate::allocator::{GlobalAllocator, Vmm};
	use crate::sync::{LazyLock, RwSpinlock};

	unsafe extern "Rust" {
		#[link_name = "__popcorn_memory_physical_highmem"]
		pub safe static GLOBAL_HIGHMEM: GlobalAllocator;

		#[link_name = "__popcorn_memory_physical_dmamem"]
		pub safe static GLOBAL_DMA: GlobalAllocator;

		#[link_name = "__popcorn_memory_virtual_kernel_global"]
		pub safe static GLOBAL_VIRTUAL_ALLOCATOR: LazyLock<RwSpinlock<&'static (dyn Vmm + Sync)>>;
	}
}

pub mod time {
	use core::num::NonZero;

	unsafe extern "Rust" {
		#[link_name = "__popcorn_system_time"]
		pub safe fn system_time() -> u128;

		#[link_name = "__popcorn_system_time_scale"]
		pub safe fn system_time_to_nanos() -> (u128, NonZero<u128>);
	}
}

pub mod executor {
	use alloc::boxed::Box;
	use core::pin::Pin;

	unsafe extern "Rust" {
		#[link_name = "__popcorn_async_spawn_task"]
		pub safe fn spawn_task(task: Pin<Box<dyn Future<Output = ()> + 'static>>);

		#[link_name = "__popcorn_async_park_inner_yield"]
		pub safe fn yield_from_async_block();
	}
}

pub mod handle {
	use alloc::sync::Arc;
	use crate::syscall;
	use crate::syscall::handle::Handle;

	unsafe extern "Rust" {
		#[link_name = "__popcorn_handle_drop"]
		pub safe fn drop(this: &mut Handle);

		#[link_name = "__popcorn_ksyscall_blocking"]
		pub safe fn kernel_syscall_blocking(
			this: &Arc<Handle>,
			protocol: u128,
			method: u32,
			args: [usize; 4]
		) -> syscall::Result<u128>;
	}
}

pub mod threading {
	use alloc::sync::Arc;
	use crate::threading::ThreadMeta;

	unsafe extern "Rust" {
		#[link_name = "__popcorn_threading_modify_current_thread_meta"]
		pub safe fn with_current_thread(arg: *mut (), f: fn(&Arc<ThreadMeta>, *mut ()));

		#[link_name = "__popcorn_threading_unblock_thread"]
		pub safe fn unblock_thread(this: &Arc<ThreadMeta>);
	}
}

pub mod address_space {
	use core::mem::MaybeUninit;
	use crate::address_space::AddressSpace;

	pub fn is_current(this: Option<&AddressSpace>) -> bool {
		let this = match this {
			Some(this) => this,
			None => return false,
		};
		let mut current = MaybeUninit::<AddressSpace>::uninit();
		crate::bridge::threading::with_current_thread(current.as_mut_ptr().cast(), |meta, out| {
			unsafe { core::ptr::write(out.cast(), meta.address_space.clone()) };
		});
		AddressSpace::ptr_eq(this, unsafe { current.assume_init_ref() })
	}

	pub mod kernel {
		use crate::mapping::{Ty, MapPageError};
		use crate::memory::{PhysicalAddress, RawFrame, RawPage, VirtualAddress};

		unsafe extern "Rust" {
			#[link_name = "__popcorn_kpt_map_contiguous"]
			pub safe fn map_contiguous(page: RawPage, frame: RawFrame, count: usize, ty: Ty, flags: u8) -> Result<(), MapPageError>;

			#[link_name = "__popcorn_kpt_unmap"]
			pub safe fn unmap(page: RawPage) -> Result<(), ()>;

			#[link_name = "__popcorn_kpt_translate_page"]
			pub safe fn translate_page(page: RawPage) -> Option<RawFrame>;

			#[link_name = "__popcorn_kpt_translate_addr"]
			pub safe fn translate_addr(addr: VirtualAddress) -> Option<PhysicalAddress>;
		}
	}

	pub mod user {
		use alloc::borrow::Cow;
		use alloc::boxed::Box;
		use crate::address_space::AddressSpace;
		use crate::allocator::Vmm;
		use crate::mapping::{MapPageError, Mappable, Mapping, Ty};
		use crate::memory::{PhysicalAddress, RawFrame, RawPage, VirtualAddress};

		unsafe extern "Rust" {
			#[link_name = "__popcorn_upt_map_contiguous"]
			pub safe fn map_contiguous(this: &AddressSpace, base_page: RawPage, base_frame: RawFrame, count: usize, ty: Ty, flags: u8) -> Result<(), MapPageError>;
			
			#[link_name = "__popcorn_upt_unmap"]
			pub safe fn unmap(this: *const (), page: RawPage) -> Result<(), ()>;

			#[link_name = "__popcorn_upt_translate_page"]
			pub safe fn translate_page(this: &AddressSpace, page: RawPage) -> Option<RawFrame>;

			#[link_name = "__popcorn_upt_translate_addr"]
			pub safe fn translate_addr(this: &AddressSpace, addr: VirtualAddress) -> Option<PhysicalAddress>;

			#[link_name = "__popcorn_address_space_push_mapping"]
			pub safe fn push_mapping<'a>(this: &'a AddressSpace, name: Cow<'static, str>, mapping: Mapping<Box<dyn Mappable + Send>, crate::address_space::Userspace>) -> (crate::address_space::MappingKey, crate::sync::MappedSpinlockGuard<'a, Mapping<Box<dyn Mappable + Send>, crate::address_space::Userspace>>);

			#[link_name = "__popcorn_address_space_get_allocator"]
			pub safe fn get_allocator(this: &AddressSpace) -> &'_ (dyn Vmm + 'static);
		}
	}
}

pub mod panicking {
	unsafe extern "Rust" {
		#[link_name = "__popcorn_print_stack_trace"]
		pub safe fn stack_trace();
	}
}
