// rust features
#![feature(custom_test_frameworks)]
#![test_runner(test_harness::test_runner)]
#![reexport_test_harness_main = "test_main"]
#![feature(const_trait_impl)]
#![feature(allocator_api)]
#![feature(type_changing_struct_update)]
#![feature(ptr_metadata)]
#![feature(type_alias_impl_trait)]
#![feature(sync_unsafe_cell)]
#![feature(arbitrary_self_types)]
#![feature(pattern)]
#![feature(slice_ptr_get)]
#![feature(min_specialization)]
#![feature(doc_cfg)]
#![feature(integer_atomics)]
#![feature(arbitrary_self_types_pointers)]
#![feature(macro_metavar_expr_concat)]
#![feature(linkage)]
#![feature(once_cell_try_insert)]
#![feature(maybe_uninit_as_bytes)]
#![feature(prelude_import)]
#![feature(super_let)]
#![feature(try_blocks)]
#![feature(derive_const)]
#![feature(const_default)]
#![feature(const_convert)]
#![cfg_attr(feature = "hal-next", feature(abi_custom))]
#![cfg_attr(feature = "hal-next", feature(integer_widen_truncate))]
#![cfg_attr(feature = "syscall-abi-next", feature(rust_preserve_none_cc))]
#![no_std]
#![no_main]

#![allow(internal_features)]

extern crate alloc;
#[cfg(panic = "unwind")]
extern crate unwinding;
extern crate kernel_api; // to pull in asan runtime

#[cfg(not(test))] use core::panic::PanicInfo;
#[cfg(kasan)] use core::ptr::NonNull;
#[cfg(kasan)] use kernel_api::allocator::Pmm;
use core::ptr::{addr_of, addr_of_mut, slice_from_raw_parts_mut};
use kernel_api::memory::{PhysicalAddress, RawFrame, RawPage, VirtualAddress};
use core::ptr;
use core::cmp::{max, min};
#[cfg(kasan)] use core::marker::PhantomData;
use core::num::NonZero;
use core::panic::AssertUnwindSafe;
use core::time::Duration;
use hashbrown::HashMap;
use elf::segment::Flags as SegmentFlags;
use hal::exception::DebugTy;
use kernel_api::{dbg, is_x86_feature_detected, mapping};
use kernel_api::mapping::Stack;
use kernel_api::ptr::LocalUser;
use kernel_api::time::Instant;
use utils::handoff::MemoryType;
use utils::handoff::MemoryMapEntry;
use crate::hal::exception::Ty;
#[cfg(kasan)] use crate::hal::paging2::Flags;
use crate::hal::paging2::{KTable, TTable};
use kernel_api::syscall::handle::Handle;
use crate::ipc::protocol::Protocol;
use crate::memory::paging::ktable;
use crate::memory::watermark_allocator::WatermarkAllocator;
use crate::panicking::SymbolMap;

//mod sync;
mod memory;
mod panicking;
mod logging;
mod bridge;
//mod task;
mod threading;
mod bmp;
mod hal;
mod timing;
//mod mmio;
mod interrupts;
mod ipc;
mod io_ext;
mod percpu;
mod loader;
mod notes;
#[cfg(feature = "hal-next")]
mod arch;
#[cfg(feature = "syscall-abi-next")]
mod syscall;
mod ebr;

// The compiler expects the prelude definition to be defined before it's use statement
mod prelude;
#[prelude_import]
#[allow(unused_imports)]
pub use prelude::*;

#[cfg(test)]
pub mod test_harness;

fn get_foo() -> usize {
	unsafe { *percpu::percpu_v2!(foo).get() }
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

#[macro_export]
macro_rules! non_zero {
    ($num:tt) => {
        const {
            match ::core::num::NonZero::new($num) {
                Some(x) => x,
                None => panic!("Cannot use `0` as a NonZero constant"),
            }
        }
    };
}

#[macro_export]
macro_rules! hashmap_new {
    () => { HashMap::with_hasher(::hashbrown::hash_map::DefaultHashBuilder::new()) };
}

#[macro_export]
macro_rules! assert_unsafe_precondition {
    ($message:expr, ($($name:ident:$ty:ty = $arg:expr),*$(,)?) => $e:expr $(,)?) => {
        #[cfg(debug_assertions)] {
            #[inline]
            /* todo: const */ fn precondition_check($($name:$ty),*) {
                if !$e {
                    panic!(
                        concat!("unsafe precondition(s) violated: ", $message)
                    );
                }
            }

            precondition_check($($arg,)*);
        }
    };
}

#[derive(Debug)]
#[repr(C)]
pub struct SyscallStack {
	flags: usize,
	ip: usize,
}

#[inline]
extern "C" fn syscall_handler(
	a: usize,
	b: usize,
	c: usize,
	d: usize,
	e: usize,
	num_high: usize,
	num_low: usize,
	stack: *mut SyscallStack,
	async_data: usize,
) -> u128 {
	//threading::exit_trampoline(move || {
		debug!("foo{num_high:#x}");
	//});

	threading::exit_trampoline(move || {
		let flags = unsafe { addr_of!((*stack).flags).read_volatile() };
		let ip = unsafe { addr_of!((*stack).ip).read_volatile() };
		let is_async = (flags & 1) != 0;
		let is_async_text = if is_async { "async " } else { "" };
		let async_key = if is_async { Some(async_data) } else { None };
		let tid = percpu::percpu_v2!(current_thread)
				.read()
				.as_ref()
				.map(|tcb| tcb.thread_id);

		let protocol = ((num_high & 0xFFFFFFFF) as u128) << 96 | (num_low as u128);
		let method = (num_high >> 32) as u32;

		#[cfg(debug_assertions)] let syscall = ipc::protocol::syscall_name(protocol, method);
		#[cfg(not(debug_assertions))] let syscall = format!("{:#x}@{:024x}", method, protocol);

		debug!("{is_async_text}syscall({syscall}, {a:#x}, {b:#x}, {c:#x}, {d:#x}, {e:#x}) @ {ip:#x} on {tid:?}");

		let syscall_result = ipc::entry(
			protocol,
			method,
			a, b, c, d, e,
			async_key,
		);

		dbg!(flags);
		if syscall_result.is_err() {
			unsafe { addr_of_mut!((*stack).flags).write_volatile(flags | 1) };
		} else {
			unsafe { addr_of_mut!((*stack).flags).write_volatile(flags & !1) };
		};
		let stack = unsafe { stack.read_volatile() };
		debug!("return stack: {:#x?}", stack);

		debug!("result for {is_async_text}syscall({syscall}, {a:#x}, {b:#x}, {c:#x}, {d:#x}, {e:#x}) @ {ip:#x} on {:?}", percpu::percpu_v2!(current_thread).read().as_ref().unwrap().thread_id);
		dbg!(syscall_result).unwrap_or_else(|v| v as u128)
	})
}

#[inline]
fn exception_handler(exception: &mut hal::exception::Exception) {
	let mut exception = AssertUnwindSafe(exception);
	threading::exit_trampoline(move || {
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
				if !exception.user_mode {
					error!("Kernel exception occurred at {at:#x} - {}:\n{ty}", panicking::get_symbol_from_ip(at).name);
					backtrace();
					loop {}
				} else {
					let tid = percpu::percpu_v2!(current_thread)
							.read()
							.as_ref()
							.map(|tcb| tcb.thread_id);

					error!("Userspace exception occurred at {at:#x} {tid:?}:\n{ty}");
					sprintln!("{:#x?}", exception.registers);
					todo!()
				}
			},
			ty @ Ty::PageFault(fault) => {
				// todo: check for CoW etc.
				let phys = || if fault.access_addr.is_higher_half() {
					ktable().translate_address(fault.access_addr, true)
				} else {
					percpu::percpu_v2!(current_thread)
							.try_read()?
							.as_ref()?
							.address_space.ttable().translate_address(fault.access_addr, true)
				};

				if !exception.user_mode {
					error!("Kernel page fault occurred at {at:#x} - {}:\n{ty}", panicking::get_symbol_from_ip(at).name);

					dbg!(fault.meta.present());
					dbg!(fault.access_addr >= kernel_api::memory::asan::SHADOW_MAP_START);
					dbg!(fault.access_addr < kernel_api::memory::asan::SHADOW_MAP_END);
					dbg!(phys());
					#[cfg(kasan)]
					if !fault.meta.present()
							&& fault.access_addr >= kernel_api::memory::asan::SHADOW_MAP_START
							&& fault.access_addr < kernel_api::memory::asan::SHADOW_MAP_END {
						use kernel_api::allocator::highmem;

						debug!("allocating extra shadow mem");
						match highmem().allocate_one_raw() {
							Ok(frame) => {
								let page = fault.access_addr.align_down_to_page();

								ktable().map_page(
									page,
									frame,
									mapping::Ty::SHADOW_MEM,
									Flags::WRITE,
								).expect("we just got a page fault for this page being not preset");

								unsafe {
									kernel_api::memory::asan::set_shadow_uninit_vmem(
										*page,
										4096 / 8,
									);
								}

								debug!("new shadow mem added");

								return;
							},
							Err(_) => {
								error!("failed to lazy allocate shadow mem");
								backtrace();
								loop {}
							}
						}
					}

					backtrace();

					unsafe extern "C" {
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
					let tid = percpu::percpu_v2!(current_thread)
							.read()
							.as_ref()
							.map(|tcb| tcb.thread_id);
					error!("Userspace page fault occurred at {at:#x} {tid:?}:\n{ty}");
					sprintln!("{:#x?}", exception.registers);
					dbg!(phys());
					todo!()
				}
			}
			ty @ (Ty::Nmi | Ty::Panic) => {
				// todo: BSOD equivalent?
				error!("Unhandled exception occurred at {at:#x} - {}:\n{ty}", panicking::get_symbol_from_ip(at).name);
				if !exception.user_mode { backtrace(); }
				loop {}
			},
			ty @ Ty::Debug(DebugTy::Breakpoint) => {
				warn!("Breakpoint: {at:#x} - {}:\n{ty}", panicking::get_symbol_from_ip(at).name);
				if !exception.user_mode { backtrace(); }
			},
			ty @ Ty::Unknown(_) => {
				warn!("Ignoring exception at {at:#x} - {}:\n{ty}", panicking::get_symbol_from_ip(at).name);
				if !exception.user_mode { backtrace(); }
			},
		}
	})
}

mod handoff {
	use core::marker::PhantomData;
	use core::ptr::NonNull;
	use kernel_api::memory::asan::no_asan_shim;
	use utils::handoff::MemoryMapEntry;
	use crate::panicking;
	use crate::panicking::SymbolMap;
	use core::ops::Range;
	use kernel_api::dbg;
	use kernel_api::memory::{PhysicalAddress, RawFrame, RawPage, VirtualAddress};
	use crate::hal::{KTableTy, TTableTy};

	#[derive(Clone, Debug)]
	pub struct MemoryMapIter<'data> {
		map: Range<*const MemoryMapEntry>,
		_phantom: PhantomData<&'data [MemoryMapEntry]>,
	}

	unsafe impl Send for MemoryMapIter<'_> {}

	impl Iterator for MemoryMapIter<'_> {
		type Item = MemoryMapEntry;

		fn next(&mut self) -> Option<Self::Item> {
			let this = self;
			no_asan_shim!(|this: &mut MemoryMapIter<'_>| -> Option<MemoryMapEntry> {
				if this.map.is_empty() { return None; }
				let item = unsafe { *this.map.start };
				this.map.start = this.map.start.wrapping_add(1);
				Some(item)
			})
		}
	}

	impl DoubleEndedIterator for MemoryMapIter<'_> {
		fn next_back(&mut self) -> Option<Self::Item> {
			let this = self;
			no_asan_shim!(|this: &mut MemoryMapIter<'_>| -> Option<MemoryMapEntry> {
				if this.map.is_empty() { return None; }
				this.map.end = this.map.end.wrapping_sub(1);
				Some(unsafe { *this.map.end })
			})
		}
	}

	pub struct ParsedHandoff<'data> {
		pub memory_map: MemoryMapIter<'data>,
		pub max_vmem: RawPage,
		pub rsdp: PhysicalAddress,
		pub framebuffer: utils::handoff::Framebuffer,
		pub stack: utils::handoff::Stack,
		pub bootloader_utable: TTableTy,
		pub init_utable: TTableTy,
		pub init_entry: VirtualAddress,
	}

	pub fn process_handoff(handoff_data: &*const utils::handoff::Data) -> ParsedHandoff<'_> {
		debug!("parsing handoff data");
		let (s_table, u_tables) = no_asan_shim!(|handoff_data: &*const utils::handoff::Data| -> (RawFrame, [RawFrame; 2]) {
			unsafe { (
				(**handoff_data).memory.s_table,
				(**handoff_data).memory.u_tables,
			) }
		});
		#[cfg(feature = "hal-next")] {
			let s_table = unsafe { KTableTy::from_raw(dbg!(s_table)) };
			unsafe { crate::memory::paging::init_page_table(s_table) };
		}
		let bootloader_utable = unsafe { TTableTy::from_raw(u_tables[0]) };
		let init_utable = unsafe { TTableTy::from_raw(u_tables[1]) };

		let symbol_map = no_asan_shim!(|handoff_data: &*const utils::handoff::Data| -> Option<NonNull<[u8]>> {
			unsafe { (**handoff_data).log.symbol_map }
		});
		*panicking::SYMBOL_MAP.write() = SymbolMap::from(symbol_map);

		no_asan_shim!(|handoff_data: &*const utils::handoff::Data, bootloader_utable: TTableTy, init_utable: TTableTy| -> ParsedHandoff<'_> {
			let core::range::Range { start, end } = unsafe { (**handoff_data).memory.map };
			let memory_map = MemoryMapIter {
				map: start.to_virtual().as_ptr().cast_const().cast::<MemoryMapEntry>()..end.to_virtual().as_ptr().cast_const().cast::<MemoryMapEntry>(),
				_phantom: PhantomData,
			};

			let max_vmem = unsafe { (**handoff_data).memory.lowest_used };
			let rsdp = unsafe { (**handoff_data).rsdp };
			let framebuffer = unsafe { (**handoff_data).framebuffer };
			let stack = unsafe { (**handoff_data).memory.stack };
			let init_entry = unsafe { (**handoff_data).init_entry };

			ParsedHandoff {
				memory_map,
				max_vmem,
				rsdp,
				framebuffer,
				stack,
				bootloader_utable,
				init_utable,
				init_entry,
			}
		})
	}
}

#[unsafe(export_name = "_start")]
extern "sysv64" fn kmain(handoff_data: *const utils::handoff::Data) -> ! {
	sprintln!("POP");

	let _ = logging::init();

	percpu::Percpu::bsp_init();

	let x = get_foo();
	assert_eq!(x, 6, "TLS value should be 6");

	#[cfg(feature = "hal-next")] arch::target_bsp_start();

	hal::early_init();

	let parsed_handoff = handoff::process_handoff(&handoff_data);
	let free_memory = parsed_handoff.memory_map
		.filter(|entry|
			entry.ty == MemoryType::Free ||
			entry.ty == MemoryType::BootloaderCode ||
			// technically this is still referenced until pmm initialisation
			// however allocations only occur after this iterator is read
			// during pmm init, so by the time allocations occur, this will
			// be free
			entry.ty == MemoryType::BootloaderData ||
			// we don't yet support UEFI runtime services
			entry.ty == MemoryType::RuntimeCode ||
			entry.ty == MemoryType::RuntimeData
		);

	// Split allocator system is used when a significant portion of memory is above the 4GiB boundary
	// This allows better optimization for non-DMA allocations as well as reducing pressure on memory usable by DMA
	// The current algorithm uses split allocators when the total amount of non-DMA memory is >= 1GiB
	let split_allocators = if cfg!(not(target_pointer_width = "32")) {
		const FOUR_GB: PhysicalAddress = PhysicalAddress::new(1<<32);

		let bytes_over_4gb: usize = free_memory.clone()
				.filter(|entry| entry.start() >= FOUR_GB)
				.map(|entry| entry.end() - entry.start())
				.sum();

		bytes_over_4gb >= 1024*1024*1024
	} else { false };

	info!("Split allocator: {}", if split_allocators { "enabled" } else { "disabled" });

	{
		if split_allocators {
			todo!("split allocators not supported yet :(");
		}

		let max_usable_memory = free_memory.clone()
			.map(MemoryMapEntry::end)
			.max()
			.expect("Free memory should exist");

		// we need handoff data to stick around until the non-bootstrap allocator is initialised,
		// so skip loader data in the bootstrap allocator
		let mut bootstrap_spaces = free_memory.clone()
		                              .filter_map(|entry| {
			                              (entry.ty != MemoryType::BootloaderData).then_some(
			                                entry.start().align_up_to_frame() .. entry.end().align_down_to_frame()
			                              )
		                              });

		let all_spaces = free_memory
			.map(|entry| {
				entry.start().align_up_to_frame() .. entry.end().align_down_to_frame()
			});

		let watermark_allocator = WatermarkAllocator::new(&mut bootstrap_spaces);

		debug!("Initialising highmem");

		let allocator = unsafe {
			memory::physical::with_highmem_as(&watermark_allocator, || unsafe {
				bitmap_allocator::BitmapAllocator::new(
					RawFrame::new(0)..max_usable_memory.align_down_to_frame(),
					all_spaces,
				).expect("unable to create highmem")
			})
		};

		memory::physical::init_highmem(allocator);
		memory::physical::init_dmamem(allocator);

		let btree_alloc = linked_list_allocator::LinkedListAllocator::new(
			RawPage::new(kernel_api::memory::asan::SHADOW_MAP_END.addr)..parsed_handoff.max_vmem,
		).unwrap();

		debug!("btree_alloc = {btree_alloc:x?}");

		*memory::r#virtual::GLOBAL_VIRTUAL_ALLOCATOR.write() = Box::leak(Box::new(btree_alloc));
	}

	debug!("initializing epoch structures");
	ebr::init();

	unsafe { hal::acpi::init_tables(parsed_handoff.rsdp) };

	let (mut update_line, _picos_per_tick) = {
		let fb = parsed_handoff.framebuffer;
		let size = fb.stride * fb.height;
		let stride = fb.stride;
		let fb_data = unsafe { &mut *slice_from_raw_parts_mut(fb.buffer.cast::<u32>(), size) };

		// Clear to true black to match background of BGRT logo
		for pixel in fb_data.iter_mut() {
			*pixel = 0;
		}

		// If extracted from BGRT, draw OEM logo
		if let Some(bgrt) = hal::acpi::tables().find_table::<::acpi::sdt::bgrt::Bgrt>() {
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
		(update_line, picos_per_tick)
	};


	hal::post_acpi_init();

	// all bootloader data structures parsed so safe to switch to PID0 utable and drop bootloader
	// SAFETY: `init_utable` ownership transferred into PID0 TCB
	unsafe { parsed_handoff.init_utable.load() };
	drop(parsed_handoff.bootloader_utable);

	let init_thread = threading::init(parsed_handoff.stack, parsed_handoff.init_utable);
	debug!("Init running on {init_thread:?}");

	let _animation = move || {
		let mut next_time = Instant::now();
		loop {
			next_time += Duration::from_millis(500); //Duration::from_nanos(1302083);
			update_line();
			kernel_api::executor::block_on(threading::sleep_until(next_time));
		}
	};

	/*let task = threading::spawn_kernel(animation, Cow::Borrowed("Boot animation")).unwrap();
	debug!("Boot animation running on {task:?}");*/

	threading::debug();
	threading::yield_now();

	hal::switch_to_userspace_at(parsed_handoff.init_entry, VirtualAddress::new(0));
}

#[cfg(not(test))]
#[panic_handler]
fn panic_handler(info: &PanicInfo) -> ! {
	sprint!("\u{001b}[31m\u{001b}[1mPANIC:");
	if let Some(location) = info.location() {
		sprint!(" {location}");
	}
	if let Some(current_thread) = percpu_v2!(current_thread).try_read()
			&& let Some(current_thread) = current_thread.as_ref() {
		sprint!(" on thread {:?}", current_thread.thread_id);
	}
	sprintln!("\u{001b}[0m");

	sprintln!("{}", info.message());

	panicking::stack_trace();
	panicking::do_panic()
}

mod allocator {
	use core::alloc::{AllocError, GlobalAlloc, Layout};
	use core::ptr;
	use core::ptr::NonNull;
	use log::trace;

	unsafe extern "Rust" {
		fn __popcorn_kernel_heap_allocate(layout: Layout) -> Result<NonNull<u8>, AllocError>;
		fn __popcorn_kernel_heap_deallocate(ptr: NonNull<u8>, layout: Layout);
		fn __popcorn_kernel_heap_reallocate(ptr: NonNull<u8>, layout: Layout, new_size: usize) -> Result<NonNull<u8>, AllocError>;
	}
	
	mod private {
		use core::ptr::NonNull;
		use core::alloc::{AllocError, Layout};
		use core::ptr;
		use super::{__popcorn_kernel_heap_allocate, __popcorn_kernel_heap_deallocate};
		
		extern crate arena_heap;

		#[unsafe(no_mangle)]
		#[linkage = "weak"]
		fn __popcorn_kernel_heap_reallocate(ptr: NonNull<u8>, layout: Layout, new_size: usize) -> Result<NonNull<u8>, AllocError> {
			let new_layout = unsafe { Layout::from_size_align_unchecked(new_size, layout.align()) };
			
			let new_ptr = unsafe { __popcorn_kernel_heap_allocate(new_layout)? };
			unsafe { ptr::copy_nonoverlapping(ptr.as_ptr(), new_ptr.as_ptr(), core::cmp::min(layout.size(), new_size)); }
			unsafe { __popcorn_kernel_heap_deallocate(ptr, layout); }
			
			Ok(new_ptr)
		}
	}

	struct HookAllocator;

	unsafe impl GlobalAlloc for HookAllocator {
		unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
			trace!("alloc({layout:?})");

			if log::max_level() >= log::LevelFilter::Trace && layout.size() > 2*kernel_api::memory::PAGE_SIZE {
				crate::panicking::stack_trace();
			}

			let ptr = match unsafe { __popcorn_kernel_heap_allocate(layout) } {
				Ok(ptr) => {
					assert_unsafe_precondition!(
						"pointer returned by `alloc` not valid for layout",
						(ptr: *mut u8 = ptr.as_ptr(), layout: Layout = layout) => ptr.align_offset(layout.align()) == 0,
					);

					ptr.as_ptr()
				},
				Err(_) => ptr::null_mut()
			};
			trace!("alloc({layout:?}) = {ptr:#p}");
			ptr
		}

		unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
			trace!("dealloc({layout:?}, {ptr:#p})");
			match NonNull::new(ptr) {
				Some(ptr) => unsafe { __popcorn_kernel_heap_deallocate(ptr, layout) },
				None => {}
			}
		}

		unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
			trace!("realloc({layout:?},{ptr:#p},{new_size})");
			let new_ptr = match NonNull::new(ptr) {
				Some(ptr) => {
					match unsafe { __popcorn_kernel_heap_reallocate(ptr, layout, new_size) } {
						Ok(ptr) => ptr.as_ptr(),
						Err(_) => ptr::null_mut()
					}
				},
				None => ptr::null_mut(),
			};
			trace!("realloc({layout:?},{ptr:#p},{new_size}) = {new_ptr:#p}");
			new_ptr
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
