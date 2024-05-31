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
#![feature(sync_unsafe_cell)]
#![feature(arbitrary_self_types)]
#![feature(pattern)]
#![feature(slice_ptr_len)]
#![feature(slice_ptr_get)]
#![feature(map_try_insert)]
#![feature(coroutines)]
#![feature(coroutine_trait)]
#![feature(str_from_raw_parts)]
#![feature(build_hasher_default_const_new)]
#![feature(pattern_types)]
#![feature(core_pattern_type)]

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
#![feature(kernel_time)]

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
use core::ptr::{addr_of, addr_of_mut, slice_from_raw_parts_mut};
use log::{debug, error, info, trace, warn};
use kernel_api::memory::{AllocError, mapping, Page, PhysicalAddress, VirtualAddress};
use core::{future, mem, ptr};
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
use handoff_protection::HandoffWrapper;
use hal::exception::DebugTy;

mod sync;
mod memory;
mod panicking;
mod logging;
mod bridge;
mod task;
mod threading;
mod bmp;
mod hal;
mod timing;
mod projection;
mod mmio;
mod interrupts;
mod ipc;

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

#[macro_export]
macro_rules! yeet {
    ($e:expr) => {return Err($e);};
}

#[inline]
fn syscall_handler() {

}

#[inline]
fn exception_handler(exception: &mut hal::exception::Exception) {
	// todo: update this to signal userspace
	let is_kernel_mode = true;
	
	let backtrace = || {
		sprintln!("---");
		sprintln!("{:#x?}", exception.registers);
		sprintln!("---");
		panicking::stack_trace();
		sprintln!("---");
	};

	let at = exception.registers.ip();

	match &exception.ty {
		// Signalling exceptions
		ty @ (Ty::FloatingPoint | Ty::IllegalInstruction | Ty::BusFault | Ty::Generic(_)) => {
			if is_kernel_mode {
				error!("Kernel exception occurred at {:#x} - {}:\n{ty}", at, panicking::get_symbol_name(at));
				backtrace();
				loop {}
			} else {
				todo!()
			}
		},
		ty @ Ty::PageFault(_) => {
			// todo: check for CoW etc.
			if is_kernel_mode {
				error!("Kernel page fault occurred at {:#x} - {}:\n{ty}", at, panicking::get_symbol_name(at));
				backtrace();

				extern "C" {
					static __popcorn_deref_handlers_check_start: u64;
					static __popcorn_deref_handlers_handle_start: u64;
					static __popcorn_deref_handlers_end: u64;
				}

				let checkpoints = unsafe {
					core::slice::from_raw_parts(
						addr_of!(__popcorn_deref_handlers_check_start),
						addr_of!(__popcorn_deref_handlers_handle_start).offset_from(addr_of!(__popcorn_deref_handlers_check_start)) as usize
					)
				};
				
				if let Some(idx) = checkpoints.iter().map(|x| *x as usize).position(|x| x == at) {
					let jumppoints = unsafe {
						core::slice::from_raw_parts(
							addr_of!(__popcorn_deref_handlers_handle_start),
							addr_of!(__popcorn_deref_handlers_end).offset_from(addr_of!(__popcorn_deref_handlers_handle_start)) as usize
						)
					};
					let jump = jumppoints.iter().map(|x| *x as usize).nth(idx).expect("Malformed deref jumptable");
					debug!("Checked access - jumping to {jump:#x}");
					exception.registers.set_ip(jump);
					return;
				}

				loop {}
			} else {
				todo!()
			}
		}
		ty @ (Ty::Nmi | Ty::Panic) => {
			// todo: BSOD equivalent?
			error!("Unhandled exception occurred at {:#x} - {}:\n{ty}", at, panicking::get_symbol_name(at));
			if is_kernel_mode { backtrace(); }
			loop {}
		},
		ty @ Ty::Debug(DebugTy::Breakpoint) => {
			warn!("Breakpoint: {:#x} - {}:\n{ty}", at, panicking::get_symbol_name(at));
			if is_kernel_mode { backtrace(); }
		},
		ty @ Ty::Unknown(_) => {
			warn!("Ignoring exception at {:#x} - {}:\n{ty}", at, panicking::get_symbol_name(at));
			if is_kernel_mode { backtrace(); }
		},
	}
}

mod handoff_protection {
	use core::fmt::{Debug, Formatter};
	use core::ops::Deref;
	use derive_more::Constructor;
	use crate::hal::{Hal, HalTy};

	#[derive(Constructor)]
	pub struct HandoffWrapper(&'static utils::handoff::Data, <HalTy as Hal>::TTableTy);

	impl HandoffWrapper {
		pub fn to_empty_ttable(self) -> <HalTy as Hal>::TTableTy {
			// todo!("empty the ttable");
			self.1
		}
	}

	impl Deref for HandoffWrapper {
		type Target = utils::handoff::Data;

		fn deref(&self) -> &Self::Target {
			self.0
		}
	}

	impl Debug for HandoffWrapper {
		fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
			self.0.fmt(f)
		}
	}
}

#[export_name = "_start"]
extern "sysv64" fn kstart(handoff_data: &'static utils::handoff::Data) -> ! {
	sprintln!("Hello world!");

	let ttable = unsafe {
		use memory::paging::init_page_table;

		let (ktable, ttable) = construct_tables();

		init_page_table(ktable);
		ttable
	};

	#[cfg(not(test))] kmain(HandoffWrapper::new(handoff_data, ttable));
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
use kernel_api::time::Instant;
use crate::hal::paging2::{construct_tables, TTable, TTableTy};
use utils::handoff::MemoryType;
use crate::hal::acpi::XPhysicalMapping;
use crate::hal::exception::{PageFault, Ty};
use crate::memory::paging::ktable;
use crate::memory::watermark_allocator::WatermarkAllocator;
use crate::task::executor::Executor;

fn kmain(handoff_data: HandoffWrapper) -> ! {
	let _ = logging::init();

	let map = unsafe { handoff_data.log.symbol_map.map(|ptr| &*ptr.as_ptr().byte_add(0xffff_8000_0000_0000)) };
	*panicking::SYMBOL_MAP.write() = map;

	trace!("Handoff data:\n{handoff_data:x?}");

	HalTy::early_init();

	let usable_memory = handoff_data.memory.map.iter().filter(|entry|
		entry.ty == MemoryType::Free || entry.ty == MemoryType::BootloaderCode
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

	let (mut update_line, picos_per_tick) = if let Some(ref fb) = handoff_data.framebuffer {
		let size = fb.stride * fb.height;
		let stride = fb.stride;
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

		let mut draw_hline = move |startx: usize, endx: usize, y: usize, c| {
			for x in startx..endx {
				fb_data[x + y*stride] = c;
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

	let tls_data_size = handoff_data.tls.0.end() - handoff_data.tls.0.start();
	let tls_data_aligned_size = {
		let align = handoff_data.tls.1;
		assert!(align.is_power_of_two(), "TLS alignment must be a power of 2");
		let mask = align - 1;
		let tls_aligned_data_size =
				if (tls_data_size & mask) == 0 { tls_data_size }
				else {
					(tls_data_size | mask) + 1
				};
		tls_aligned_data_size
	};
	let tls_size = tls_data_aligned_size + mem::size_of::<*mut u8>();
	// Is this always correctly aligned?
	#[warn(deprecated)]
			let tls = OldMapping::new(tls_size.div_ceil(4096))
			.expect("Unable to allocate TLS area");
	let (tls, _) = tls.into_raw_parts();
	debug!("TLS starts at {tls:x?}");
	debug!("TLS size is {tls_data_size:#x};{tls_size:#x}");
	unsafe {
		core::ptr::copy_nonoverlapping(handoff_data.tls.0.start().as_ptr(), tls.as_ptr(), tls_data_size);
		let tls_self_ptr = tls.as_ptr().byte_add(tls_size - mem::size_of::<*mut u8>());
		info!("Placing pointer to self at {tls_self_ptr:p}");
		tls_self_ptr.cast::<*mut u8>().write(tls_self_ptr);
		HalTy::load_tls(tls_self_ptr);
	}

	let x = get_foo();
	assert_eq!(x, 6, "TLS value should be 6");

	if let Ok(hpet) = ::acpi::hpet::HpetInfo::new(hal::acpi::tables()) {
		unsafe { hal::arch::hpet::Hpet::init(hpet, hal::acpi::Handler::new(&hal::acpi::Allocator)); }
	}

	<HalTy as Hal>::post_acpi_init();

	let init_thread = unsafe { threading::init(handoff_data) };
	debug!("{init_thread:x?}");

	{
		let update_line = update_line.as_mut().map(|f| f as &mut dyn FnMut());

		if let Some(mut update_line) = update_line {
			use crate::hal::timing::{Timer, Eoi};

			extern "C" fn animation_task((data, meta): (usize, usize)) -> ! {
				let update_line = unsafe { &mut *core::ptr::from_raw_parts_mut::<dyn FnMut()>(data as *mut (), mem::transmute(meta)) };
				let mut next_time = Instant::now();
				loop {
					next_time += Duration::from_nanos(1302083);
					update_line();
					threading::sleep_until(next_time);
				}
			}

			let update_line_parts = (update_line as *mut dyn FnMut()).to_raw_parts();

			{
				let ttable = TTableTy::new(&*ktable(), highmem()).unwrap();
				let task = ThreadControlBlock::new(
					Cow::Borrowed("Boot animation"),
					ttable,
					threading::thread_startup,
					animation_task,
					(update_line_parts.0 as _, unsafe { mem::transmute(update_line_parts.1) })
				);
				let mut guard = threading::scheduler::SCHEDULER.lock();
				guard.add_task(task);
			}
		}
	}

	/*{
		let ttable = TTableTy::new(&*ktable(), highmem()).unwrap();
		let task = ThreadControlBlock::new(
			Cow::Borrowed("PS/2 driver"),
			ttable,
			threading::thread_startup,
			drivers::i8042::main,
			()
		);
		let mut guard = threading::scheduler::SCHEDULER.lock();
		guard.add_task(task);
	}*/

	{
		const CORE_SOCKET_OPEN: u128 = 0;

		let shim = |a: &str| {
			let a = a.as_bytes();
			debug!("{}", ipc::syscall_entry(CORE_SOCKET_OPEN, a.as_ptr() as _, a.len(), 0, 0));
		};
		shim("hello world!");
		shim("hello.foo.world.:/byee/eee");
		shim(".:/byee/eee");
		shim(":/byee/");
		shim(":byee/");
		debug!("{}", ipc::syscall_entry(CORE_SOCKET_OPEN, ptr::null::<u8>() as _, 10, 0, 0));
		debug!("{}", ipc::syscall_entry(CORE_SOCKET_OPEN, 0xdeadbeef, 10, 0, 0));
		shim(":core.input.mouse@");
		{
			extern "C" fn f(_: ()) -> ! {
				let a = "core.input.mouse@:mouse".as_bytes();
				debug!("{}", ipc::syscall_entry(CORE_SOCKET_OPEN, a.as_ptr() as _, a.len(), 0, 0));
				threading::exit(0)
			}
			let ttable = TTableTy::new(&*ktable(), highmem()).unwrap();
			let task = ThreadControlBlock::new(
				Cow::Borrowed("foo"),
				ttable,
				threading::thread_startup,
				f,
				()
			);
			let mut guard = threading::scheduler::SCHEDULER.lock();
			guard.add_task(task);
		}
		threading::thread_yield();
		debug!("{:#?}", &*ipc::server::servers());
	}

	loop {
		unsafe { asm!("hlt"); }
		threading::thread_yield();
	}

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
