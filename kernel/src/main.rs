// rust features
#![feature(custom_test_frameworks)]
#![test_runner(test_harness::test_runner)]
#![reexport_test_harness_main = "test_main"]
#![feature(const_trait_impl)]
#![feature(allocator_api)]
#![feature(let_chains)]
#![feature(const_type_name)]
#![feature(decl_macro)]
#![feature(abi_x86_interrupt)]
#![feature(generic_arg_infer)]
#![feature(gen_blocks)]
#![feature(type_changing_struct_update)]
#![feature(maybe_uninit_array_assume_init)]
#![feature(inherent_associated_types)]
#![feature(pointer_like_trait)]
#![feature(int_roundings)]
#![feature(vec_into_raw_parts)]
#![feature(strict_provenance_atomic_ptr)]
#![feature(maybe_uninit_uninit_array_transpose)]
#![feature(ptr_metadata)]
#![feature(naked_functions)]
#![feature(type_alias_impl_trait)]
#![feature(sync_unsafe_cell)]
#![feature(arbitrary_self_types)]
#![feature(pattern)]
#![feature(slice_ptr_get)]
#![feature(map_try_insert)]
#![feature(coroutines)]
#![feature(coroutine_trait)]
#![feature(str_from_raw_parts)]
#![feature(min_specialization)]
#![feature(doc_auto_cfg)]
#![feature(integer_atomics)]
#![feature(arbitrary_self_types_pointers)]
#![feature(macro_metavar_expr_concat)]
#![feature(linkage)]
#![feature(once_cell_try_insert)]
#![feature(unsigned_nonzero_div_ceil)]
#![feature(generic_const_exprs)]
#![feature(pointer_is_aligned_to)]
#![feature(macro_metavar_expr)]

#![feature(kernel_heap)]
#![feature(kernel_allocation_new)]
#![feature(kernel_sync_once)]
#![feature(kernel_physical_page_offset)]
#![feature(kernel_memory_addr_access)]
#![feature(kernel_virtual_memory)]
#![feature(kernel_mmap_to_parts)]
#![feature(kernel_mmap_config)]
#![feature(kernel_internals)]
#![feature(kernel_physical_allocator_non_contiguous)]
#![feature(kernel_physical_allocator_location)]
#![feature(kernel_ptr)]
#![feature(kernel_time)]
#![feature(kernel_feature_detect)]
#![feature(kernel_irq_cell)]
#![feature(kernel_allocation_zeroing)]
#![feature(kernel_mmap_trait)]

#![no_std]
#![no_main]

#![deny(deprecated)]
#![allow(refining_impl_trait)]

extern crate alloc;
#[cfg(panic = "unwind")]
extern crate unwinding;

extern crate self as kernel;

#[allow(unused_imports)] use crate::prelude::*;
use alloc::borrow::Cow;
use core::alloc::Layout;
use core::cell::UnsafeCell;
use core::panic::PanicInfo;
use core::ptr::{addr_of, slice_from_raw_parts_mut};
use kernel_api::memory::{Page, PhysicalAddress, VirtualAddress};
use core::{future, mem, ptr};
use core::cmp::{max, min};
use core::mem::ManuallyDrop;
use core::num::NonZero;
use core::task::{Poll};
use core::time::Duration;
use itertools::Itertools;
use elf::header::program::SegmentType;
use crate::threading::ThreadControlBlock;
use handoff_protection::HandoffWrapper;
use hal::exception::DebugTy;
use kernel_api::dbg;
use kernel_api::memory::{Frame};
use kernel_api::memory::allocator::{Config, SizedBackingAllocator};
use kernel_api::memory::mapping::{Mapping, self, Location, Stack, Protection, new_mapping_in, new_stack_in};
use kernel_api::memory::physical::highmem;
use kernel_api::memory::r#virtual::address_space::AddressSpace;
use kernel_api::memory::r#virtual::Userspace;
use kernel_api::ptr::{slice_from_raw_parts, User};
use kernel_api::time::Instant;
use utils::handoff::MemoryType;
use crate::hal::exception::Ty;
use crate::hal::paging2::{KTable, TTable};
use crate::ipc::handle::Handle;
use crate::ipc::protocol::Protocol;
use crate::memory::paging::ktable;
use crate::memory::r#virtual::AddressSpaceInner;
use crate::memory::watermark_allocator::WatermarkAllocator;
use crate::task::executor::Executor;
use crate::threading::{WakeTrigger, WakeReason};

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
mod prelude;
mod io_ext;
mod percpu;

#[cfg(test)]
pub mod test_harness;

fn get_foo() -> usize {
	unsafe { *percpu_v2!(foo).get() }
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
            match NonZero::new($num) {
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

#[inline]
extern "C" fn syscall_handler(
	a: usize,
	b: usize,
	c: usize,
	d: usize,
	e: usize,
	num_high: usize,
	num_low: usize,
	ip: usize,
	flags: &mut usize,
) -> u128 {
	debug!("syscall({num_high:#x}{num_low:016x}, {a:#x}, {b:#x}, {c:#x}, {d:#x}, {e:#x}) @ {ip:#x} on {:?}", percpu_v2!(current_thread).read().as_ref().unwrap().tcb_ref().thread_id);

	let syscall_result = ipc::entry(
		((num_high & 0xFFFFFFFF) as u128) << 96 | (num_low as u128),
		(num_high >> 32) as u32,
		a, b, c, d, e
	);
	
	dbg!(*flags);
	if syscall_result.is_err() { *flags |= 1 } else { *flags &= !1 };
	dbg!(*flags);
	
	dbg!(syscall_result).unwrap_or_else(|v| v as u128)
}

#[inline]
fn exception_handler(exception: &mut hal::exception::Exception) {
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
				error!("Kernel exception occurred at {:#x} - {}:\n{ty}", at, panicking::get_symbol_from_ip(at).name);
				backtrace();
				loop {}
			} else {
				error!("Userspace exception occurred at {:#x}:\n{ty}", at);
				debug!("{:#x?}", exception.registers);
				todo!()
			}
		},
		ty @ Ty::PageFault(_) => {
			// todo: check for CoW etc.
			if !exception.user_mode {
				error!("Kernel page fault occurred at {:#x} - {}:\n{ty}", at, panicking::get_symbol_from_ip(at).name);
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
				error!("Userspace page fault occurred at {:#x}:\n{ty}", at);
				debug!("{:#x?}", exception.registers);
				todo!()
			}
		}
		ty @ (Ty::Nmi | Ty::Panic) => {
			// todo: BSOD equivalent?
			error!("Unhandled exception occurred at {:#x} - {}:\n{ty}", at, panicking::get_symbol_from_ip(at).name);
			if !exception.user_mode { backtrace(); }
			loop {}
		},
		ty @ Ty::Debug(DebugTy::Breakpoint) => {
			warn!("Breakpoint: {:#x} - {}:\n{ty}", at, panicking::get_symbol_from_ip(at).name);
			if !exception.user_mode { backtrace(); }
		},
		ty @ Ty::Unknown(_) => {
			warn!("Ignoring exception at {:#x} - {}:\n{ty}", at, panicking::get_symbol_from_ip(at).name);
			if !exception.user_mode { backtrace(); }
		},
	}
}

mod handoff_protection {
	use core::fmt::{Debug, Formatter};
	use core::ops::Deref;
	use derive_more::Constructor;
	use crate::{hal, panicking};

	#[derive(Constructor)]
	pub struct HandoffWrapper(&'static utils::handoff::Data, hal::TTableTy);

	impl HandoffWrapper {
		pub fn to_empty_ttable(self) -> hal::TTableTy {
			// todo!("empty the ttable");
			*panicking::SYMBOL_MAP.write() = None; // HACK
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

		let (ktable, ttable) = hal::construct_tables();

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

fn kmain(handoff_data: HandoffWrapper) -> ! {
	let _ = logging::init();

	// fixme: lifetime here is wrong and when `handoff_data` gets dropped the symbol map is useless
	let map = handoff_data.log.symbol_map;
	*panicking::SYMBOL_MAP.write() = map;

	trace!("Handoff data:\n{handoff_data:x?}");

	hal::early_init();

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

	percpu::Percpu::init();

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

	let x = get_foo();
	assert_eq!(x, 6, "TLS value should be 6");

	/*if let Ok(hpet) = ::acpi::hpet::HpetInfo::new(hal::acpi::tables()) {
		unsafe { hal::arch::hpet::Hpet::init(hpet, hal::acpi::Handler::new(&hal::acpi::Allocator)); }
	}*/

	hal::post_acpi_init();

	let init_data = Box::from(handoff_data.init_exec);

	let init_thread = threading::init(handoff_data);
	debug!("Init running on {init_thread:?}");

	if let Some(mut update_line) = update_line {
		let animation = move || {
			let mut next_time = Instant::now();
			loop {
				next_time += Duration::from_millis(500); //Duration::from_nanos(1302083);
				update_line();
				threading::sleep_until(next_time);
			}
		};

		let task = threading::spawn_with(animation, Cow::Borrowed("Boot animation")).unwrap();
		debug!("Boot animation running on {task:?}");
	}
	threading::debug();
	threading::yield_now();

	let (entrypoint, stack) = {
		let guard = percpu_v2!(current_thread).read();
		let address_space = guard.as_ref().unwrap().tcb_ref().address_space;

		let stack_top = {
			let config = mapping::Config::new_in(NonZero::new(4).unwrap(), AddressSpaceInner::to_api(address_space))
					.virtual_location(Location::At(Page::new(VirtualAddress::new(0x40000000))))
					.protection(Protection::RWXU);

			let mut stack = new_stack_in(config, u16::MAX).unwrap();
			
			fn set_up_stack<'arg, 'env, 'handle>(
				stack: &mut Stack<Userspace>,
				arg: impl IntoIterator<Item = &'arg str>,
				env: impl IntoIterator<Item = &'env str>,
				handles: impl IntoIterator<IntoIter = impl Clone + Iterator<Item = (&'handle str, isize)>>
			) -> VirtualAddress {
				let (arg, env, handles) = (arg.into_iter(), env.into_iter(), handles.into_iter());
				let mut stack_ptr = stack.virtual_end().as_ptr();

				fn write_strings<'a>(stack_ptr: &mut *mut u8, strings: impl Iterator<Item = &'a str>) -> Vec<usize> {
					let mut ptrs = Vec::with_capacity(strings.size_hint().0);
					for str in strings {
						// write null terminator
						*stack_ptr = unsafe { stack_ptr.offset(-1) };
						unsafe { stack_ptr.write(0) };

						// write string content
						*stack_ptr = unsafe { stack_ptr.sub(str.len()) };
						unsafe { ptr::copy_nonoverlapping(str.as_bytes().as_ptr(), *stack_ptr, str.len()); }

						debug!("{:#p} = {str}", *stack_ptr);

						// store start ptr
						ptrs.push(stack_ptr.addr());
					}
					ptrs
				}

				let (handle_ids, handle_nums) = (handles.clone().map(|(s, _)| s), handles.map(|(_, i)| i as usize));
				let arg_ptrs = write_strings(&mut stack_ptr, arg);
				let env_ptrs = write_strings(&mut stack_ptr, env);
				let handle_ptrs = write_strings(&mut stack_ptr, handle_ids);

				let align_offset = stack_ptr.align_offset(size_of::<usize>());
				let mut stack_ptr = unsafe { stack_ptr.cast::<usize>().byte_sub(size_of::<usize>() - align_offset) };

				let total_count = 1 // argc
						+ arg_ptrs.len()
						+ 1 // argv terminator
						+ env_ptrs.len()
						+ 1 // envp terminator
						+ 2 // auxv terminator
						+ 2 * handle_ptrs.len() // handle map
						+ 1; // handle terminator
				stack_ptr = unsafe { stack_ptr.sub(total_count) };
				if !stack_ptr.is_aligned_to(16) { stack_ptr = unsafe { stack_ptr.sub(1) }; }

				for (offset, ptr) in core::iter::once(arg_ptrs.len()) // argc
								.chain(arg_ptrs.into_iter()) // argv
								.chain(core::iter::once(0)) // argv terminator
								.chain(env_ptrs.into_iter()) // env
								.chain(core::iter::once(0)) // env terminator
								.chain(core::iter::repeat_n(0, 2)) // auxv terminator
								.chain(handle_ptrs.into_iter().interleave_shortest(handle_nums)) // handle list
								.chain(core::iter::once(0)) // handle list terminator
						        .enumerate()
				{
					unsafe { stack_ptr.add(offset).write(ptr) };
					debug!("{:#p} = {ptr:#x}", unsafe { stack_ptr.add(offset) });
				}

				assert!(stack_ptr.is_aligned_to(16), "userspace stack pointer not 16-byte aligned");

				VirtualAddress::from(stack_ptr)
			}

			let addr = set_up_stack(
				&mut stack,
				["init", "hello", "world"],
				["LANG=en_GB.UTF-8"],
				[
					("io.stdin", 0),
					("io.stdout", 1),
					("io.stderr", 2),
				],
			);

			address_space.add_mapping("[stack@3]", stack);

			addr
		};

		let file = elf::File::try_new(&init_data).unwrap();

		for segment in file.segments()
		                   .filter(|s| s.segment_type == SegmentType::LOAD) {
			assert!(segment.alignment <= 4096, "Not designed for >1 page alignment");

			let addr = VirtualAddress::<1>::new(segment.vaddr.try_into().unwrap());
			let segment_page_offset = addr - addr.align_down::<4096>();

			let len = segment_page_offset + usize::try_from(segment.memory_size).unwrap();
			let len = len.div_ceil(4096);
			
			let mapping = {
				let config = mapping::Config::new_in(len.try_into().unwrap(), AddressSpaceInner::to_api(address_space))
						.virtual_location(Location::At(Page::new(addr.align_down())))
						.protection(Protection::RWXU);
				new_mapping_in(config, u16::MAX).unwrap()
			};

			assert!(segment.file_size <= segment.memory_size);

			unsafe {
				ptr::copy_nonoverlapping(
					file[segment.file_location()].as_ptr(),
					addr.as_ptr(),
					segment.file_size.try_into().unwrap(),
				);
				ptr::write_bytes(
					addr.as_ptr().byte_add(segment.file_size.try_into().unwrap()),
					0,
					(segment.memory_size - segment.file_size).try_into().unwrap(),
				);
			}

			address_space.add_mapping("/user/init.exec", mapping);
		}
		
		debug!("{address_space:?}");

		(VirtualAddress::new(file.entrypoint()), stack_top)
	};
	drop(init_data);
	{
		debug!("opening init stdin/out/err/thread as handles 0..=3");

		let _ = ipc::server::server_registry(); // force it to init builtin servers (root + proc)

		let guard = percpu_v2!(current_thread).read();
		let thread = guard.as_ref().unwrap().tcb_ref();

		let stdio_handle = ipc::open("console:/", [].into(), User::null()).expect("unable to open console");
		let thread_handle = Handle::new(
			ipc::server::server_registry().get_server_at("proc").expect("unable to open `proc`").0,
			thread.thread_id.get() as isize,
			&[<dyn ipc::protocol::generated::CoreProcThread>::UID]
		);

		thread.handles.openat(0, stdio_handle.clone())
				.expect("unable to open fd 0");
		thread.handles.openat(1, stdio_handle.clone())
				.expect("unable to open fd 1");
		thread.handles.openat(2, stdio_handle)
				.expect("unable to open fd 2");
		thread.handles.openat(3, thread_handle)
				.expect("unable to open fd 3");
	}
	hal::switch_to_userspace_at(entrypoint, stack);
}

#[cfg(not(test))]
#[panic_handler]
fn panic_handler(info: &PanicInfo) -> ! {
	sprint!("\u{001b}[31m\u{001b}[1mPANIC:");
	if let Some(location) = info.location() {
		sprint!(" {location}");
	}
	sprintln!("\u{001b}[0m");

	sprintln!("{}", info.message());

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
	use core::alloc::{AllocError, GlobalAlloc, Layout};
	use core::ptr;
	use core::ptr::NonNull;
	use log::debug;

	extern "Rust" {
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

		#[no_mangle]
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
			debug!("alloc({layout:?})");
			match __popcorn_kernel_heap_allocate(layout) {
				Ok(ptr) => {
					assert_unsafe_precondition!(
						"pointer returned by `alloc` not valid for layout",
						(ptr: *mut u8 = ptr.as_ptr(), layout: Layout = layout) => ptr.align_offset(layout.align()) == 0,
					);

					ptr.as_ptr()
				},
				Err(_) => ptr::null_mut()
			}
		}

		unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
			debug!("dealloc({layout:?})");
			match NonNull::new(ptr) {
				Some(ptr) => __popcorn_kernel_heap_deallocate(ptr, layout),
				None => {}
			}
		}

		unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
			debug!("realloc({layout:?},{new_size})");
			match NonNull::new(ptr) {
				Some(ptr) => {
					match __popcorn_kernel_heap_reallocate(ptr, layout, new_size) {
						Ok(ptr) => ptr.as_ptr(),
						Err(_) => ptr::null_mut()
					}
				},
				None => ptr::null_mut(),
			}
		}
	}

	#[cfg_attr(not(test), global_allocator)]
	static ALLOCATOR: HookAllocator = HookAllocator;
}

mod paging_codes {
	pub const BGRT_BMP_HEADER: u16 = 10;
	pub const IOAPIC_REGISTERS: u16 = 11;
	pub const APIC_REGISTERS: u16 = 12;
	pub const HPET_HEADER: u16 = 13;
	pub const HPET_FULL: u16 = 14;
	pub const PHYSMAP_OTHER: u16 = 15;
	pub const ACPI_SDT_HEADER: u16 = 16;
	pub const ACPI_RSDP: u16 = 17;
	pub const ACPI_HPET: u16 = 18;
	pub const ACPI_FADT: u16 = 19;
	pub const ACPI_BGRT: u16 = 20;
	pub const BYTE_ARRAY: u16 = 21;
	pub const THREAD_KERNEL_STACK: u16 = 22;
	pub const TLS: u16 = 23;
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
