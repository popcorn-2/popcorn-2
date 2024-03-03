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
#![feature(naked_functions)]
#![feature(type_alias_impl_trait)]
#![feature(asm_const)]
#![feature(const_mut_refs)]
#![feature(pointer_is_aligned)]
#![feature(sync_unsafe_cell)]
#![feature(arbitrary_self_types)]

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
#![feature(kernel_ptr)]

#![no_std]
#![no_main]

#![deny(deprecated)]

extern crate alloc;
#[cfg(panic = "unwind")]
extern crate unwinding;

extern crate self as kernel;

use alloc::borrow::Cow;
use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use core::alloc::{Allocator, GlobalAlloc, Layout};
use core::arch::asm;
use core::cell::{RefCell, UnsafeCell};
use core::fmt::Write;
use core::ops::Deref;
use core::panic::PanicInfo;
use core::ptr::{addr_of_mut, slice_from_raw_parts_mut};
use log::{debug, error, info, trace, warn};
use kernel_api::memory::{AllocError, mapping, Page, PhysicalAddress, VirtualAddress};
use core::{future, mem};
use core::cmp::{max, min};
use core::num::NonZeroUsize;
use core::task::{Poll, Waker};
use core::time::Duration;
use ::acpi::{AcpiHandler, AcpiTables, PhysicalMapping};
use ::acpi::madt::MadtEntry;
use kernel_api::memory::{allocator::BackingAllocator};
#[warn(deprecated)]
use kernel_api::memory::mapping::OldMapping;
use hal::{HalTy, Hal, ThreadControlBlock, ThreadState, SaveState};

mod sync;
mod memory;
mod panicking;
mod logging;
mod bridge;
mod task;
mod threading;
mod bmp;
mod hal;

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

#[thread_local]
static IRQ_HANDLES: Mutex<BTreeMap<usize, Box<dyn FnMut() /* + Send ???*/>>> = Mutex::new(BTreeMap::new());

#[inline]
fn irq_handler(num: usize) {
	if let Some(f) = IRQ_HANDLES.lock().get_mut(&num) {
		(*f)();
	} else {
		warn!("Unhandled IRQ num {num}");
	}
}

#[inline]
fn syscall_handler() {

}

#[inline]
fn exception_handler(exception: hal::exception::Exception) {
	// todo: update this to signal userspace
	let is_kernel_mode = true;

	match exception.ty {
		// Signalling exceptions
		ty @ (Ty::FloatingPoint | Ty::IllegalInstruction | Ty::BusFault | Ty::Generic(_)) => {
			if is_kernel_mode {
				error!("Kernel exception occurred at {:#x} - {}:\n{ty}", exception.at_instruction, panicking::get_symbol_name(exception.at_instruction));
				loop {}
			} else {
				todo!()
			}
		},
		ty @ Ty::PageFault(_) => {
			// todo: check for CoW etc.
			if is_kernel_mode {
				error!("Kernel page fault occurred at {:#x} - {}:\n{ty}", exception.at_instruction, panicking::get_symbol_name(exception.at_instruction));
				loop {}
			} else {
				todo!()
			}
		}
		ty @ (Ty::Nmi | Ty::Panic) => {
			// todo: BSOD equivalent?
			error!("Unhandled exception occurred at {:#x} - {}:\n{ty}", exception.at_instruction, panicking::get_symbol_name(exception.at_instruction));
			loop {}
		},
		ty @ (Ty::Unknown(_) | Ty::Debug(_)) => {
			warn!("Ignoring exception at {:#x} - {}:\n{ty}", exception.at_instruction, panicking::get_symbol_name(exception.at_instruction));
		}
	}
}

#[export_name = "_start"]
extern "sysv64" fn kstart(handoff_data: &'static utils::handoff::Data) -> ! {
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
use kernel_api::memory::allocator::{Config, SizedBackingAllocator, SpecificLocation};
use kernel_api::memory::mapping::Stack;
use kernel_api::memory::physical::highmem;
use kernel_api::memory::r#virtual::Global;
use kernel_api::ptr::Unique;
use kernel_api::sync::Mutex;
use crate::hal::paging2::{construct_tables, TTable, TTableTy};
use utils::handoff::MemoryType;
use crate::hal::acpi::XPhysicalMapping;
use crate::hal::exception::{PageFault, Ty};
use crate::memory::paging::ktable;
use crate::memory::watermark_allocator::WatermarkAllocator;
use crate::task::executor::Executor;

fn kmain(mut handoff_data: &'static utils::handoff::Data, ttable: TTableTy) -> ! {
	let _ = logging::init();

	let map = unsafe { handoff_data.log.symbol_map.map(|ptr| &*ptr.as_ptr().byte_add(0xffff_8000_0000_0000)) };
	*panicking::SYMBOL_MAP.write() = map;

	trace!("Handoff data:\n{handoff_data:x?}");

	HalTy::early_init();
	HalTy::breakpoint();

	let usable_memory = handoff_data.memory.map.iter().filter(|entry|
		entry.ty == MemoryType::Free
			|| entry.ty == MemoryType::BootloaderCode
			//|| entry.ty == MemoryType::BootloaderData
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

	unsafe {
		hal::acpi::init_tables(handoff_data.rsdp.addr);
	}

	let (update_line, picos_per_tick) = if let Some(ref fb) = handoff_data.framebuffer {
		let size = fb.stride * fb.height;
		let fb_data = unsafe { &mut *slice_from_raw_parts_mut(fb.buffer.as_ptr().cast::<u32>(), size) };

		// Clear to true black to match background of BGRT logo
		for pixel in fb_data.iter_mut() {
			*pixel = 0;
		}

		// If extracted from BGRT, draw OEM logo
		if let Ok(bgrt) = hal::acpi::tables().find_table::<::acpi::bgrt::Bgrt>() {
			let bitmap = bmp::from_bgrt(&bgrt, hal::acpi::Handler::new(&hal::acpi::Allocator));
			if let Some(bitmap) = bitmap {
				let width = bitmap.width as usize;

				let (x_init, y) = bgrt.image_offset();
				let x_init = x_init as usize;
				let mut y = y as usize;
				let mut x = x_init;

				for pixel in bitmap {
					fb_data[x + y*fb.stride] = pixel;
					x += 1;
					if x - x_init >= width { x = x_init; y += 1; }
				}
			}
		}

		// Draw progress bar outline
		const PROGRESS_BAR_COLOR_BG: u32 = 0x303030;
		const PROGRESS_BAR_COLOR_FG: u32 = 0xababab;

		let mut draw_hline = |startx: usize, endx: usize, y: usize, c| {
			for x in startx..endx {
				fb_data[x + y*fb.stride] = c;
			}
		};

		let progress_bar_height = ((fb.height as f32) * 0.005) as usize;
		let progress_bar_width = ((fb.width as f32) * 0.60) as isize;
		let progress_bar_start_x = ((fb.width as f32) * 0.20) as isize;
		let progress_bar_start_y = ((fb.height as f32) * 0.65) as usize;
		let mut x_offset = -(progress_bar_width/3);
		let mut direction = true;

		const ONE_WAY_TIME: Duration = Duration::from_secs(1);
		let picos_per_tick = (ONE_WAY_TIME.as_nanos() * 1000) / u128::try_from(progress_bar_width).unwrap();
		debug!("{picos_per_tick}");

		let mut update_line = move || {
			let x_start = max(
				progress_bar_start_x,
				progress_bar_start_x + x_offset
			);
			let x_end = min(
				progress_bar_start_x + progress_bar_width,
				progress_bar_start_x + x_offset + (progress_bar_width/3)
			);

			for y in progress_bar_start_y..progress_bar_start_y+progress_bar_height {
				draw_hline(progress_bar_start_x as usize, x_start as usize, y, PROGRESS_BAR_COLOR_BG);
				draw_hline(x_start as usize, x_end as usize, y, PROGRESS_BAR_COLOR_FG);
				draw_hline(x_end as usize, (progress_bar_start_x+progress_bar_width) as usize, y, PROGRESS_BAR_COLOR_BG);
			}

			if direction {
				x_offset += 1;
				if x_offset >= progress_bar_width { direction = false; }
			} else {
				x_offset -= 1;
				if x_offset <= -(progress_bar_width/3) { direction = true; }
			}
		};

		update_line();
		(Some(update_line), Some(picos_per_tick))
	} else { (None, None) };

	let tls_size = handoff_data.tls.end() - handoff_data.tls.start() + mem::size_of::<*mut u8>();
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

	{
		if let Ok(hpet) = ::acpi::hpet::HpetInfo::new(hal::acpi::tables()) {
			#[repr(C)]
			#[derive(Debug)]
			struct HpetHeader {
				capabilities: u64,
				_res0: u64,
				configuration: u64,
				_res1: u64,
				status: u64,
				_res2: [u64; 25],
				counter: u64,
				_res3: u64,
			}

			#[repr(C)]
			#[derive(Debug)]
			struct HpetTimer {
				capabilities: u64,
				comparator: u64,
				fsb_route: u64,
				_res: u64,
			}

			#[repr(C)]
			#[derive(Debug)]
			struct Hpet {
				header: HpetHeader,
				timers: [HpetTimer]
			}

			let hpet_map = unsafe { hal::acpi::Handler::new(&hal::acpi::Allocator).map_region::<HpetHeader>(hpet.base_address, mem::size_of::<HpetHeader>(), ()) };
			let hpet_timer_count = ((hpet_map.capabilities >> 8) & 0b11111) + 1;
			let hpet_size = mem::size_of::<HpetHeader>() + 0x20*(hpet_timer_count as usize);
			drop(hpet_map);
			let hpet_map = unsafe { hal::acpi::Handler::new(&hal::acpi::Allocator).map_region::<Hpet>(hpet.base_address, hpet_size, hpet_timer_count as usize) };
			debug!("{:#x?}", hpet_map.deref());
		}

		<HalTy as Hal>::post_acpi_init();

		if let Some(mut update_line) = update_line {
			use crate::hal::timing::{Timer, Eoi};

			let mut timer = <HalTy as Hal>::LocalTimer::get();
			let eoi = timer.eoi_handle();

			let tick_func = move || {
				update_line();
				eoi.send();
			};
			IRQ_HANDLES.lock().insert(48, Box::new(tick_func));

			let time_period = timer.get_time_period_picos().unwrap() * 4;
			timer.set_irq_number(48).unwrap();
			timer.set_divisor(4).unwrap();
			debug!("{time_period:?} {} {}", picos_per_tick.unwrap(), picos_per_tick.unwrap() / u128::from(time_period));
			timer.start_periodic(picos_per_tick.unwrap() / u128::from(time_period)).unwrap();
		}
	}

	let x = get_foo();
	warn!("TLS value is {x}");

	loop {}

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

	loop {}

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
