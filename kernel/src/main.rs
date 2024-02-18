// rust features
#![feature(custom_test_frameworks)]
#![test_runner(test_harness::test_runner)]
#![reexport_test_harness_main = "test_main"]
#![feature(const_trait_impl)]
#![feature(allocator_api)]
#![feature(let_chains)]
#![feature(specialization)]
#![feature(const_type_name)]
#![feature(inline_const)]
#![feature(decl_macro)]
#![feature(abi_x86_interrupt)]
#![feature(generic_arg_infer)]
#![feature(panic_info_message)]
#![feature(gen_blocks)]
#![feature(maybe_uninit_uninit_array)]
#![feature(type_changing_struct_update)]
#![feature(maybe_uninit_array_assume_init)]
#![feature(dyn_star)]
#![feature(inherent_associated_types)]
#![feature(generic_const_exprs)]
#![feature(pointer_like_trait)]
#![feature(exclusive_range_pattern)]
#![feature(int_roundings)]
#![feature(thread_local)]
#![feature(noop_waker)]
#![feature(vec_into_raw_parts)]
#![feature(strict_provenance_atomic_ptr)]
#![feature(maybe_uninit_uninit_array_transpose)]
#![feature(ptr_metadata)]

#![feature(kernel_heap)]
#![feature(kernel_allocation_new)]
#![feature(kernel_sync_once)]
#![feature(kernel_physical_page_offset)]
#![feature(kernel_memory_addr_access)]
#![feature(kernel_virtual_memory)]
#![feature(kernel_mmap)]
#![feature(kernel_internals)]
#![feature(kernel_physical_allocator_v2)]
#![feature(kernel_physical_allocator_non_contiguous)]
#![feature(kernel_physical_allocator_location)]

#![no_std]
#![no_main]

#![deny(deprecated)]

extern crate alloc;
#[cfg(panic = "unwind")]
extern crate unwinding;

extern crate self as kernel;

use alloc::borrow::Cow;
use alloc::boxed::Box;
use core::alloc::{Allocator, GlobalAlloc, Layout};
use core::arch::asm;
use core::cell::UnsafeCell;
use core::fmt::Write;
use core::ops::Deref;
use core::panic::PanicInfo;
use core::ptr::slice_from_raw_parts_mut;
use log::{debug, info, trace, warn};
use kernel_api::memory::{mapping, Page, PhysicalAddress, VirtualAddress};
use core::future;
use core::num::NonZeroUsize;
use core::task::{Poll, Waker};
use kernel_api::memory::{allocator::BackingAllocator};
#[warn(deprecated)]
use kernel_api::memory::mapping::OldMapping;
use kernel_hal::{HalTy, Hal, ThreadControlBlock, ThreadState, SaveState};

pub use kernel_hal::{sprint, sprintln};

mod sync;
mod memory;
mod panicking;
mod logging;
mod bridge;
mod task;
mod threading;
mod acpi;

#[cfg(test)]
pub mod test_harness;

#[thread_local]
static FOO: UnsafeCell<usize> = UnsafeCell::new(6);

fn get_foo() -> usize {
	unsafe { *FOO.get() }
}

#[macro_export]
macro_rules! usize {
    ($stuff:expr) => {usize::try_from($stuff).unwrap()};
}

#[macro_export]
macro_rules! u64 {
    ($stuff:expr) => {u64::try_from($stuff).unwrap()};
}

#[macro_export]
macro_rules! into {
    ($stuff:expr) => {($stuff).try_into().unwrap()};
}

#[export_name = "_start"]
extern "sysv64" fn kstart(handoff_data: &utils::handoff::Data) -> ! {
	sprintln!("Hello world!");

	let ttable = unsafe {
		use memory::paging::{init_page_table};

		let (ktable, ttable) = construct_tables();

		init_page_table(ktable);
		ttable
	};

	#[cfg(not(test))] kmain(handoff_data, ttable);
	#[cfg(test)] {
		let mut spaces = handoff_data.memory.map.iter().filter(|entry|
				entry.ty == MemoryType::Free
						|| entry.ty == MemoryType::AcpiReclaim
						|| entry.ty == MemoryType::BootloaderCode
						|| entry.ty == MemoryType::BootloaderData
		).map(|entry| {
            Frame::new(entry.start().align_up())..Frame::new(entry.end().align_down())
        });

		let mut watermark_allocator = memory::watermark_allocator::WatermarkAllocator::new(&mut spaces);
		memory::physical::with_highmem_as(&mut watermark_allocator, || test_main());

		unreachable!("test harness returned")
	}
}

use kernel_api::memory::{Frame};
use kernel_api::memory::allocator::{Config, SizedBackingAllocator};
use kernel_api::memory::mapping::Stack;
use kernel_api::memory::physical::highmem;
use kernel_api::memory::r#virtual::Global;
use kernel_hal::paging2::{construct_tables, TTable, TTableTy};
use utils::handoff::MemoryType;
use crate::memory::paging::ktable;
use crate::memory::watermark_allocator::WatermarkAllocator;
use crate::task::executor::Executor;

fn kmain(mut handoff_data: &utils::handoff::Data, ttable: TTableTy) -> ! {
	let _ = logging::init();

	let map = unsafe { handoff_data.log.symbol_map.map(|ptr| &*ptr.as_ptr().byte_add(0xffff_8000_0000_0000)) };
	*panicking::SYMBOL_MAP.write() = map;

	trace!("Handoff data:\n{handoff_data:x?}");

	HalTy::early_init();
	HalTy::init_idt();
	HalTy::breakpoint();

	let usable_memory = handoff_data.memory.map.iter().filter(|entry|
		entry.ty == MemoryType::Free
			|| entry.ty == MemoryType::AcpiReclaim
			|| entry.ty == MemoryType::BootloaderCode
			|| entry.ty == MemoryType::BootloaderData
	);

	// Split allocator system is used when a significant portion of memory is above the 4GiB boundary
	// This allows better optimization for non-DMA allocations as well as reducing pressure on memory usable by DMA
	// The current algorithm uses split allocators when the total amount of non-DMA memory is >= 1GiB
	let split_allocators = if cfg!(not(target_pointer_width = "32")) {
		const FOUR_GB: PhysicalAddress = PhysicalAddress::new(1<<32);

		let bytes_over_4gb: usize = usable_memory.clone()
				.filter(|entry| entry.start() >= FOUR_GB)
				.map(|entry| entry.end() - entry.start())
				.sum();

		bytes_over_4gb >= 1024*1024*1024
	} else { false };

	info!("Split allocator: {}", if split_allocators { "enabled" } else { "disabled" });

	panicking::stack_trace();

	{
		use kernel_api::memory::PhysicalAddress;

		if split_allocators {
			todo!("split allocators not supported yet :(");
		}

		let max_usable_memory = usable_memory.clone()
			.max_by(|a, b| a.end().cmp(&b.end()))
			.expect("Free memory should exist");
		let max_usable_memory = max_usable_memory.end();

		let mut spaces = usable_memory.clone()
			.map(|entry| {
				Frame::new(entry.start().align_up())..Frame::new(entry.end().align_down())
			});

		let mut spaces2 = spaces.clone();
		let watermark_allocator = WatermarkAllocator::new(&mut spaces2);

		debug!("Initialising highmem");

		let allocator = memory::physical::with_highmem_as(&watermark_allocator, || {
			<bitmap_allocator::Wrapped as SizedBackingAllocator>::new(
				Config {
					allocation_range: Frame::new(PhysicalAddress::new(0))..Frame::new(max_usable_memory.align_down()),
					regions: &mut spaces
				}
			)
		});

		watermark_allocator.drain_into(allocator);
		memory::physical::init_highmem(allocator);
		memory::physical::init_dmamem(allocator);

		let btree_alloc = {
			use core::iter::Iterator;

			const PAGE_MAP_OFFSET: usize = 0xffff_8000_0000_0000;
			const PAGE_MAP_OFFSET_LEN: usize = 2usize.pow(46);

			let mut btree_alloc = ranged_btree_allocator::RangedBtreeAllocator::new(
				// unfortunately this means a page is missing :(
				Page::new(VirtualAddress::new(PAGE_MAP_OFFSET+PAGE_MAP_OFFSET_LEN))..Page::new(VirtualAddress::new(0xffff_ffff_ffff_f000))
			);

			let virtual_reserved = [
				// entire bootstrap region in case adding allocations uses more heap
				// realisation: this will now cause an OOM on all subsequent heap allocations
				Page::new(VirtualAddress::new(memory::r#virtual::VMEM_BOOTSTRAP_START.0 as usize))..Page::new(VirtualAddress::new(memory::r#virtual::VMEM_BOOTSTRAP_END.0 as usize)),

				Page::new(handoff_data.memory.used.start())..Page::new(handoff_data.memory.used.end())
			].into_iter();

			btree_alloc.add_allocations(virtual_reserved);

			btree_alloc
		};

		debug!("btree_alloc = {btree_alloc:x?}");

		*memory::r#virtual::GLOBAL_VIRTUAL_ALLOCATOR.write() = Box::leak(Box::new(btree_alloc));
	}

	if let Some(ref fb) = handoff_data.framebuffer {
		let size = fb.stride * fb.height;
		for pixel in unsafe { &mut *slice_from_raw_parts_mut(fb.buffer.cast::<u32>(), size) } {
			*pixel = 0xeeeeee;
		}
	}

	let tls_size = handoff_data.tls.end() - handoff_data.tls.start() + core::mem::size_of::<*mut u8>();
	// Is this always correctly aligned?
	#[warn(deprecated)]
	let tls = OldMapping::new(tls_size.div_ceil(4096))
			.expect("Unable to allocate TLS area");
	let (tls, _) = tls.into_raw_parts();
	unsafe {
		core::ptr::copy_nonoverlapping(handoff_data.tls.start().as_ptr(), tls.as_ptr(), tls_size - core::mem::size_of::<*mut u8>());
		let tls_self_ptr = tls.as_ptr().byte_add(tls_size - core::mem::size_of::<*mut u8>());
		tls_self_ptr.cast::<*mut u8>().write(tls_self_ptr);
		HalTy::load_tls(tls_self_ptr);
	}

	let x = get_foo();
	warn!("TLS value is {x}");

	let init_thread = unsafe { threading::init(&handoff_data.memory.stack, ttable) };
	debug!("{init_thread:x?}");

	fn foo() -> ! {
		sprintln!("hello from foo!");

		threading::thread_yield();

		unreachable!()
	}

	fn bar() {
		unsafe {
			threading::scheduler::SCHEDULER.unlock();
		}
	}

	{
		let ttable = TTableTy::new(&*ktable(), highmem()).unwrap();
		let task = ThreadControlBlock::new(
			Cow::Borrowed("hellooo"),
			ttable,
			bar,
			foo,
		);
		let mut guard = threading::scheduler::SCHEDULER.lock();
		guard.add_task(task);
	}

	threading::thread_yield();

	let mut executor = Executor::new();
	static mut WAKER: Option<Waker> = None;

	executor.spawn(|| async {
		sprintln!("inside async fn, about to wait");

		let mut x = 0;
		let waiter = future::poll_fn(|ctx| {
			unsafe { WAKER = Some(ctx.waker().clone()); }
			if x < 5 { sprintln!("{x}"); x += 1; Poll::Pending }
			else { Poll::Ready(()) }
		});
		waiter.await;

		sprintln!("async fn back");
	});

	executor.spawn(|| async { for _ in 0..5 {
		sprintln!("Inside other async fn");
		unsafe { WAKER.as_ref().unwrap().wake_by_ref(); }
	}});

	executor.run();
}

#[cfg(not(test))]
#[panic_handler]
fn panic_handler(info: &PanicInfo) -> ! {
	sprint!("\u{001b}[31m\u{001b}[1mPANIC:");
	if let Some(location) = info.location() {
		sprint!(" {location}");
	}
	sprintln!("\u{001b}[0m");

	if let Some(message) = info.message() {
		sprintln!("{}", *message);
	} else if let Some(payload) = info.payload().downcast_ref::<&'static str>() {
		sprintln!("{}", payload);
	}

	panicking::do_panic()
}

#[no_mangle]
pub extern "Rust" fn __popcorn_module_panic(info: &PanicInfo) -> ! {
	panic!("Panic from module: {info}");
}

#[no_mangle]
pub unsafe extern "Rust" fn __popcorn_module_alloc(layout: Layout) -> *mut u8 {
	alloc::alloc::alloc(layout)
}

#[no_mangle]
pub unsafe extern "Rust" fn __popcorn_module_dealloc(ptr: *mut u8, layout: Layout) {
	alloc::alloc::dealloc(ptr, layout);
}

#[no_mangle]
pub unsafe extern "Rust" fn __popcorn_module_alloc_zeroed(layout: Layout) -> *mut u8 {
	alloc::alloc::alloc_zeroed(layout)
}

#[no_mangle]
pub unsafe extern "Rust" fn __popcorn_module_realloc(ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
	alloc::alloc::realloc(ptr, layout, new_size)
}

#[no_mangle]
pub unsafe extern "Rust" fn __popcorn_module_is_panicking() -> bool { panicking::panicking() }


mod allocator {
	use core::alloc::{GlobalAlloc, Layout};
	use core::ptr;
	use core::ptr::NonNull;
	use log::{debug, trace};
	use kernel_api::memory::{AllocError, heap::Heap};

	struct HookAllocator;

	unsafe impl GlobalAlloc for HookAllocator {
		unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
			debug!("alloc({layout:?})");
			match kernel_default_heap::__popcorn_kernel_heap_allocate(layout) {
				Ok(ptr) => ptr.as_ptr(),
				Err(_) => ptr::null_mut()
			}
		}

		unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
			match NonNull::new(ptr) {
				Some(ptr) => kernel_default_heap::__popcorn_kernel_heap_deallocate(ptr, layout),
				None => {}
			}
		}
	}

	#[cfg_attr(not(test), global_allocator)]
	static ALLOCATOR: HookAllocator = HookAllocator;
}

#[cfg(test)]
mod tests {
	#[test]
	fn trivial_assertion() {
		assert_eq!(1, 1);
	}

	#[test]
	fn trivial_result() -> Result<(), u8> {
		Ok(())
	}

	#[test]
	#[should_panic]
	fn trivial_failing_assertion() {
		assert_eq!(1, 3);
	}
}
