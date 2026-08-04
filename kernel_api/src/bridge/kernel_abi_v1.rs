/// Control over CPU interrupt state.
pub mod irq {
	unsafe extern "Rust" {
		/// Set the interrupt mask of the current CPU to `state`.
		///
		/// The meaning of the value of `state` is architecture defined.
		#[link_name = "__popcorn_set_irq"]
		pub safe fn set(state: usize);
		
		/// Disables interrupts on the current CPU, returning a mask that can be passed to [`set()`] to restore the original state.
		#[link_name = "__popcorn_disable_irq"]
		pub safe fn disable() -> usize;
	}
}

/// Kernel memory allocation.
pub mod memory {
	use crate::allocator::{GlobalAllocator, Vmm};
	use crate::sync::{LazyLock, RwSpinlock};

	unsafe extern "Rust" {
		/// The current `highmem` allocator.
		#[link_name = "__popcorn_memory_physical_highmem"]
		pub safe static GLOBAL_HIGHMEM: GlobalAllocator;

		/// The current `dmamem` allocator.
		#[link_name = "__popcorn_memory_physical_dmamem"]
		pub safe static GLOBAL_DMA: GlobalAllocator;

		/// The current virtual allocator.
		#[link_name = "__popcorn_memory_virtual_kernel_global"]
		pub safe static GLOBAL_VIRTUAL_ALLOCATOR: LazyLock<RwSpinlock<&'static (dyn Vmm + Sync)>>;
	}
}

/// Timekeeping.
pub mod time {
	use core::num::NonZero;

	unsafe extern "Rust" {
		/// Get the system time in an architecture defined format.
		#[link_name = "__popcorn_system_time"]
		pub safe fn system_time() -> u128;

		/// Get the conversion factors from the architecture time format to nanoseconds.
		///
		/// The first `u128` is the numerator and the second is the denominator of the factor
		/// to scale by to get nanoseconds.
		#[link_name = "__popcorn_system_time_scale"]
		pub safe fn system_time_to_nanos() -> (u128, NonZero<u128>);
	}
}

/// Async task execution.
pub mod executor {
	use core::pin::Pin;

	unsafe extern "Rust" {
		/// Spawns a task to run in the background on the kernel's async executor.
		#[link_name = "__popcorn_async_spawn_task"]
		pub safe fn spawn_task(task: Pin<Box<dyn Future<Output = ()> + 'static>>);

		/// Yield from the current thread, parking it if the current thread state is [`NearlyParked`](`crate::threading::ThreadState::NearlyParked`).
		#[link_name = "__popcorn_async_park_inner_yield"]
		pub safe fn yield_from_async_block();
	}
}

/// Kernel resource handles and system calls.
pub mod handle {
	use alloc::sync::Arc;
	use crate::syscall;
	use crate::syscall::handle::Handle;

	unsafe extern "Rust" {
		/// Sends `destruct@abi_v1` to all servers that own the passed handle.
		#[link_name = "__popcorn_handle_drop"]
		pub safe fn drop(this: &mut Handle);

		/// Executes a syscall on the passed handle, blocking the current thread until completion.
		#[link_name = "__popcorn_ksyscall_blocking"]
		pub safe fn kernel_syscall_blocking(
			this: &Arc<Handle>,
			protocol: u128,
			method: u32,
			args: [usize; 4]
		) -> syscall::Result<u128>;
	}
}

/// Kernel thread control.
pub mod threading {
	use alloc::sync::Arc;
	use core::mem::{ManuallyDrop, MaybeUninit};
	use crate::threading::ThreadMeta;

	/// Runs the passed closure, passing it a reference to the [`ThreadMeta`] for the currently
	/// running thread.
	pub fn with_current_thread<F: FnOnce(&Arc<ThreadMeta>) -> R, R>(f: F) -> R {
		#![expect(clippy::shadow_unrelated, reason = "false positive since it doesn't understand the data gets passed to the closure")]
		let out = MaybeUninit::<R>::uninit();
		let mut tup = (ManuallyDrop::new(f), out);

		with_current_thread_inner((&raw mut tup).cast(), |meta, val| {
			// SAFETY: `val` is equal to the `&mut tup` passed as the arg
			let (f, out) = unsafe { val.cast::<(ManuallyDrop<F>, MaybeUninit<R>)>().as_mut_unchecked() };
			// SAFETY: ownership of `f` has now been moved into this closure and is not dropped outside it
			let f = unsafe { ManuallyDrop::take(f) };
			let res = f(meta);
			out.write(res);
		});

		// SAFETY: initialized by closure passed to `with_current_thread_inner`
		unsafe { tup.1.assume_init() }
	}

	unsafe extern "Rust" {
		#[link_name = "__popcorn_threading_modify_current_thread_meta"]
		safe fn with_current_thread_inner(arg: *mut (), f: fn(&Arc<ThreadMeta>, *mut ()));

		/// Places the passed thread back on the scheduler's run-queue.
		#[link_name = "__popcorn_threading_unblock_thread"]
		pub safe fn unblock_thread(this: &Arc<ThreadMeta>);
	}
}

/// Address space modification.
pub mod address_space {
	use crate::address_space;

	/// Check if the passed [`address_space::User`] is the same as the current address space.
	pub fn is_current(this: Option<&address_space::User>) -> bool {
		let Some(this) = this else { return false; };

		let current = crate::bridge::threading::with_current_thread(|meta| {
			meta.address_space.clone()
		});

		this == current
	}

	/// Kernel address space.
	pub mod kernel {
		use crate::mapping::Ty;
		use crate::memory::{PhysicalAddress, RawFrame, RawPage, VirtualAddress};
		use crate::address_space::MapPageError;

		unsafe extern "Rust" {
			/// Attempts to make a mapping in the kernel page table.
			///
			/// Attempts to map the pages from `page` to `page + count` to the frames from `frame` to `frame + count`,
			/// marking it with `ty` type metadata, and with architecture specific `flags`.
			#[link_name = "__popcorn_kpt_map_contiguous"]
			pub safe fn map_contiguous(page: RawPage, frame: RawFrame, count: usize, ty: Ty, flags: u8) -> Result<(), MapPageError>;

			/// Unmaps a page from the kernel page table.
			#[expect(dead_code, reason = "not implemented yet")]
			#[link_name = "__popcorn_kpt_unmap"]
			pub safe fn unmap(page: RawPage) -> Result<(), ()>;

			/// Gets the frame mapped to `page` in the kernel page table.
			#[expect(dead_code, reason = "not implemented yet")]
			#[link_name = "__popcorn_kpt_translate_page"]
			pub safe fn translate_page(page: RawPage) -> Option<RawFrame>;

			/// Gets the addr mapped to `addr` in the kernel page table.
			#[expect(dead_code, reason = "not implemented yet")]
			#[link_name = "__popcorn_kpt_translate_addr"]
			pub safe fn translate_addr(addr: VirtualAddress) -> Option<PhysicalAddress>;
		}
	}

	/// Userspace address spaces.
	pub mod user {
		use alloc::borrow::Cow;
		use crate::address_space::{self, MapPageError};
		use crate::allocator::Vmm;
		use crate::mapping::{Mappable, Mapping, Ty};
		use crate::memory::{PhysicalAddress, RawFrame, RawPage, VirtualAddress};

		unsafe extern "Rust" {
			/// Attempts to make a mapping in a userspace page table.
			///
			/// Attempts to map the pages from `page` to `page + count` to the frames from `frame` to `frame + count`,
			/// marking it with `ty` type metadata, and with architecture specific `flags`.
			#[link_name = "__popcorn_upt_map_contiguous"]
			pub safe fn map_contiguous(this: &address_space::User, base_page: RawPage, base_frame: RawFrame, count: usize, ty: Ty, flags: u8) -> Result<(), MapPageError>;

			/// Unmaps a page from a userspace page table.
			#[expect(dead_code, reason = "not implemented yet")]
			#[link_name = "__popcorn_upt_unmap"]
			pub safe fn unmap(this: *const (), page: RawPage) -> Result<(), ()>;

			/// Gets the frame mapped to `page` in the specified page table.
			#[expect(dead_code, reason = "not implemented yet")]
			#[link_name = "__popcorn_upt_translate_page"]
			pub safe fn translate_page(this: &address_space::User, page: RawPage) -> Option<RawFrame>;

			/// Gets the address mapped to `addr` in the specified page table.
			#[link_name = "__popcorn_upt_translate_addr"]
			pub safe fn translate_addr(this: &address_space::User, addr: VirtualAddress) -> Option<PhysicalAddress>;

			#[link_name = "__popcorn_address_space_push_mapping"]
			pub safe fn push_mapping<'a>(this: &'a address_space::User, name: Cow<'static, str>, mapping: Mapping<Box<dyn Mappable + Send>, crate::address_space::User>) -> (crate::address_space::MappingKey, crate::sync::MappedSpinlockGuard<'a, Mapping<Box<dyn Mappable + Send>, crate::address_space::User>>);

			#[link_name = "__popcorn_address_space_get_allocator"]
			pub safe fn get_allocator(this: &address_space::User) -> &'_ (dyn Vmm + 'static);
		}
	}
}

/// Panicking related utilities.
pub mod panicking {
	unsafe extern "Rust" {
		/// Print a stack trace.
		#[link_name = "__popcorn_print_stack_trace"]
		pub safe fn stack_trace();
	}
}
