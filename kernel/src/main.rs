// rust features
#![feature(custom_test_frameworks)]
#![test_runner(test_harness::test_runner)]
#![reexport_test_harness_main = "test_main"]
#![feature(const_trait_impl)]
#![feature(allocator_api)]
#![feature(const_type_name)]
#![feature(decl_macro)]
#![feature(abi_x86_interrupt)]
#![feature(gen_blocks)]
#![feature(type_changing_struct_update)]
#![feature(maybe_uninit_array_assume_init)]
#![feature(int_roundings)]
#![feature(maybe_uninit_uninit_array_transpose)]
#![feature(ptr_metadata)]
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
#![feature(doc_cfg)]
#![feature(integer_atomics)]
#![feature(arbitrary_self_types_pointers)]
#![feature(macro_metavar_expr_concat)]
#![feature(linkage)]
#![feature(once_cell_try_insert)]
#![feature(pointer_is_aligned_to)]
#![feature(macro_metavar_expr)]
#![feature(bstr)]
#![feature(array_try_map)]
#![feature(vec_push_within_capacity)]
#![feature(context_ext)]
#![feature(local_waker)]
#![feature(maybe_uninit_as_bytes)]
#![feature(extern_types)]
#![feature(prelude_import)]
#![feature(super_let)]
#![feature(try_blocks)]
#![feature(sanitize)]
#![feature(derive_const)]
#![feature(const_default)]
#![feature(const_convert)]

#![no_std]
#![no_main]

#![allow(internal_features)]
#![deny(warnings)]

extern crate alloc;
#[cfg(panic = "unwind")]
extern crate unwinding;
extern crate kernel_api; // to pull in asan runtime

#[cfg(not(test))] use core::panic::PanicInfo;
#[cfg(feature = "kasan")] use core::ptr::NonNull;
#[cfg(feature = "kasan")] use kernel_api::allocator::Pmm;
use core::ptr::{addr_of, addr_of_mut, slice_from_raw_parts_mut};
use kernel_api::memory::{PhysicalAddress, RawFrame, RawPage, VirtualAddress};
use core::ptr;
use core::cmp::{max, min};
#[cfg(feature = "kasan")] use core::marker::PhantomData;
use core::num::NonZero;
use core::panic::AssertUnwindSafe;
use core::time::Duration;
use hashbrown::HashMap;
use elf::header::program::{SegmentFlags, SegmentType};
use handoff_protection::HandoffWrapper;
use hal::exception::DebugTy;
use kernel_api::{dbg, is_x86_feature_detected, mapping};
use kernel_api::mapping::Stack;
use kernel_api::ptr::LocalUser;
use kernel_api::time::Instant;
use utils::handoff::MemoryType;
#[cfg(feature = "kasan")] use utils::handoff::MemoryMapEntry;
use crate::hal::exception::Ty;
#[cfg(feature = "kasan")] use crate::hal::paging2::Flags;
use crate::hal::paging2::KTable;
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
					percpu::percpu_v2!(@current_thread)?
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
					#[cfg(feature = "kasan")]
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

mod handoff_protection {
	use core::ops::Deref;
	use derive_more::Constructor;
	use crate::{hal, panicking};
	use crate::panicking::SymbolMap;

	#[derive(Constructor)]
	pub struct HandoffWrapper(*const utils::handoff::Data, hal::TTableTy);

	impl HandoffWrapper {
		pub fn to_empty_ttable(self) -> hal::TTableTy {
			// todo!("empty the ttable");
			*panicking::SYMBOL_MAP.write() = SymbolMap::from(None); // fixme: HACK
			self.1
		}
	}

	impl Deref for HandoffWrapper {
		type Target = *const utils::handoff::Data;

		fn deref(&self) -> &Self::Target {
			&self.0
		}
	}
}

#[unsafe(export_name = "_start")]
extern "sysv64" fn kstart(handoff_data: *const utils::handoff::Data) -> ! {
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
	sprintln!("logging initialised");

	#[cfg(not(feature = "kasan"))] {
		let map = unsafe { (**handoff_data).log.symbol_map };
		*panicking::SYMBOL_MAP.write() = SymbolMap::from(map);
		
		sprintln!("Handoff data:\n{:x?}", unsafe { &**handoff_data });
	}
	#[cfg(feature = "kasan")] {
		#[sanitize(address = "off")]
		#[inline(never)]
		fn no_sanitizer_shim(data: &HandoffWrapper) -> Option<NonNull<[u8]>> {
			unsafe {
				(***data).log.symbol_map
			}
		}
		let map = no_sanitizer_shim(&handoff_data);
		
		*panicking::SYMBOL_MAP.write() = SymbolMap::from(map);
	}

	hal::early_init();
	
	#[cfg(feature = "kasan")] let usable_memory = {
		#[sanitize(address = "off")]
		#[inline(never)]
		fn no_sanitizer_shim(data: &HandoffWrapper) -> impl DoubleEndedIterator<Item = MemoryMapEntry> + Clone + '_ {
			#[derive(Clone)]
			struct Iter<'data> {
				map: core::ops::Range<*const MemoryMapEntry>,
				_phantom: PhantomData<&'data HandoffWrapper>,
			}
			
			unsafe impl Send for Iter<'_> {}
			
			impl Iterator for Iter<'_> {
				type Item = MemoryMapEntry;

				#[sanitize(address = "off")]
				#[inline(never)]
				fn next(&mut self) -> Option<Self::Item> {
					if self.map.is_empty() { return None; }
					let item = unsafe { *self.map.start };
					self.map.start = unsafe { self.map.start.offset(1) };
					Some(item)
				}
			}

			impl DoubleEndedIterator for Iter<'_> {
				#[sanitize(address = "off")]
				#[inline(never)]
				fn next_back(&mut self) -> Option<Self::Item> {
					if self.map.is_empty() { return None; }
					let item = unsafe { *self.map.end.offset(-1) };
					self.map.end = unsafe { self.map.end.offset(-1) };
					Some(item)
				}
			}
			
			unsafe {
				let map = (***data).memory.map.as_ptr_range();
				Iter {
					map,
					_phantom: PhantomData
				}
			}
		}
		
		no_sanitizer_shim(&handoff_data)
	};

	#[cfg(not(feature = "kasan"))] let usable_memory = unsafe { (**handoff_data).memory.map.iter() };
	let usable_memory = usable_memory.filter(|entry|
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
		if split_allocators {
			todo!("split allocators not supported yet :(");
		}

		let max_usable_memory = usable_memory.clone()
		                                     .max_by(|a, b| a.end().cmp(&b.end()))
		                                     .expect("Free memory should exist");
		let max_usable_memory = max_usable_memory.end();

		let mut spaces = usable_memory.clone()
		                              .map(|entry| {
			                              entry.start().align_up_to_frame() .. entry.end().align_down_to_frame()
		                              });

		let mut spaces2 = spaces.clone();
		let watermark_allocator = WatermarkAllocator::new(&mut spaces2);

		debug!("Initialising highmem");

		let allocator = memory::physical::with_highmem_as(&watermark_allocator, || unsafe {
			bitmap_allocator::Wrapped::new(
				RawFrame::new(0)..max_usable_memory.align_down_to_frame(),
				&mut spaces,
			).expect("unable to create highmem")
		});

		memory::physical::init_highmem(allocator);
		memory::physical::init_dmamem(allocator);

		let btree_alloc = {
			let mut btree_alloc = ranged_btree_allocator::RangedBtreeAllocator::new(
				// unfortunately this means a page is missing :(
				RawPage::new(kernel_api::memory::asan::SHADOW_MAP_END.addr) ..RawPage::new(memory::r#virtual::vmem_bootstrap_end() as usize)
			).unwrap();

			let virtual_reserved = [
				// entire bootstrap region in case adding allocations uses more heap
				//Page::new(VirtualAddress::new(memory::r#virtual::vmem_bootstrap_start() as usize))..Page::new(VirtualAddress::new(memory::r#virtual::vmem_bootstrap_end() as usize)),
				// ^ vmem is now included in the handoff used struct

				{
					let used = if cfg!(not(feature = "kasan")) { unsafe { (**handoff_data).memory.used } }
						else {
							#[sanitize(address = "off")]
							#[inline(never)]
							fn no_sanitizer_shim(data: &HandoffWrapper) -> utils::handoff::Range<RawPage> {
								unsafe {
									(***data).memory.used
								}
							}
							no_sanitizer_shim(&handoff_data)
						};
					used.start()..used.end()
				}
			].into_iter();
			debug!("virtual_reserved = {virtual_reserved:#x?}");

			btree_alloc.add_allocations(virtual_reserved);

			btree_alloc
		};

		debug!("btree_alloc = {btree_alloc:x?}");

		*memory::r#virtual::GLOBAL_VIRTUAL_ALLOCATOR.write() = Box::leak(Box::new(btree_alloc));
	}
	drop(usable_memory);

	percpu::Percpu::init();

	let rsdp = unsafe {
		if cfg!(not(feature = "kasan")) {
			(**handoff_data).rsdp.addr
		} else {
			#[sanitize(address = "off")]
			#[inline(never)]
			fn no_sanitizer_shim(data: &HandoffWrapper) -> usize {
				unsafe {
					(***data).rsdp.addr
				}
			}
			no_sanitizer_shim(&handoff_data)
		}
	};
	unsafe { hal::acpi::init_tables(rsdp) };

	let fb = if cfg!(not(feature = "kasan")) { unsafe { (**handoff_data).framebuffer } }
		else {
			#[sanitize(address = "off")]
			#[inline(never)]
			fn no_sanitizer_shim(data: &HandoffWrapper) -> Option<utils::handoff::Framebuffer> {
				unsafe {
					(***data).framebuffer
				}
			}
			no_sanitizer_shim(&handoff_data)
		};

	let (update_line, _picos_per_tick) = if let Some(fb) = fb {
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
	
	let init_data = if cfg!(not(feature = "kasan")) {
		unsafe { Box::<[_]>::from((**handoff_data).init_exec) }
	} else {
		#[sanitize(address = "off")]
		#[inline(never)]
		fn no_sanitizer_shim(data: &HandoffWrapper) -> Box<[u8]> {
			unsafe {
				let data = (***data).init_exec;
				let mut ret = Box::<[u8]>::new_zeroed_slice(data.len());
				let core::ops::Range { start: mut start_dest, end: end_dest } = (&mut *ret).as_mut_ptr_range();
				let core::ops::Range { start: mut start_src, .. } = data.as_ptr_range();
				while start_dest != end_dest {
					// do it this way instead of calls to memcpy or similar so that it's all contained in the no_sanitize function
					*start_dest.cast() = *start_src;
					
					start_src = start_src.offset(1);
					start_dest = start_dest.offset(1);
				}
				ret.assume_init()
			}
		}
		no_sanitizer_shim(&handoff_data)
	};

	let ramdisk_data = if cfg!(not(feature = "kasan")) {
		unsafe { Box::<[_]>::from((**handoff_data).ramdisk) }
	} else {
		#[sanitize(address = "off")]
		#[inline(never)]
		fn no_sanitizer_shim(data: &HandoffWrapper) -> Box<[u8]> {
			unsafe {
				let data = (***data).ramdisk;
				let mut ret = Box::<[u8]>::new_zeroed_slice(data.len());
				let core::ops::Range { start: mut start_dest, end: end_dest } = (&mut *ret).as_mut_ptr_range();
				let core::ops::Range { start: mut start_src, .. } = data.as_ptr_range();
				while start_dest != end_dest {
					// do it this way instead of calls to memcpy or similar so that it's all contained in the no_sanitize function
					*start_dest.cast() = *start_src;

					start_src = start_src.offset(1);
					start_dest = start_dest.offset(1);
				}
				ret.assume_init()
			}
		}
		no_sanitizer_shim(&handoff_data)
	};
	let ramdisk_server = ipc::init_ramdisk(ramdisk_data, PhysicalAddress::new(rsdp));

	let init_thread = threading::init(handoff_data);
	debug!("Init running on {init_thread:?}");

	if let Some(mut update_line) = update_line {
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
	}
	threading::debug();
	threading::yield_now();

	/*threading::spawn_kernel(|| {
		threading::sleep(Duration::from_secs(2));
		let dangly_ptr = User::<*const u8>::new_in(
			ptr::dangling(),
			AddressSpaceInner::to_api(
				percpu_v2!(current_thread).read().as_ref().expect("can only syscall from thread")
				                          .tcb_ref().address_space
			)
		);

		let handle = ipc::open(
			"fs:/foo.txt",
			&[<dyn ipc::protocol::generated::CoreIoRead>::UID, <dyn ipc::protocol::generated::CoreIoSeek>::UID],
			dangly_ptr,
		);
		let handle = dbg!(handle).unwrap();
		let res = percpu_v2!(current_thread).read().as_ref()
		                                    .unwrap()
		                                    .tcb_ref().handles.push(handle).unwrap();

		let mut buf = [0u8; 16];
		let buf_ptr = buf.as_mut_ptr();
		let buf_size = 16;

		let res = ipc::entry(
			<dyn ipc::protocol::generated::CoreIoRead>::UID,
			1,
			res as usize,
			buf_ptr as usize,
			buf_size,
			0, 0
		);
		if let Ok(bytes) = res {
			let buf = ByteStr::new(&buf[..(bytes as usize)]);
			info!("read bytes {} from `/foo.txt`", &buf)
		}
	}, Cow::Borrowed("fs tester")).unwrap();*/

	let (entrypoint, stack) = {
		let guard = percpu::percpu_v2!(current_thread).read();
		let address_space = &guard
				.as_ref()
				.expect("init must be in a thread")
				.address_space;

		let stack_top = {
			let config = mapping::Config::new(NonZero::new(8).unwrap(), mapping::Ty::USER_STACK)
					.protection(true, false, true)
					.virtual_location(RawPage::new(0x7fff_f000_0000));

			let (_, mut stack) = config.map_in::<Stack>("[stack:3]".into(), address_space)
					.expect("failed to allocate stack");

			let stack_top = loader::set_up_stack(
				&mut stack,
				["init", "hello", "world"],
				["LANG=en_GB.UTF-8", "MLIBC_DEBUG_MALLOC=0"],
				HashMap::<&'static str, u32>::from([
					("io.stdin", 0),
					("io.stdout", 1),
					("io.stderr", 2),
					("thread.main", 3),
					("popcorn.init.ramdisk", 4),
					("popcorn.init.root-bus-descriptor", 5),
				])
			);
			
			stack_top
		};

		let file = elf::File::try_new(&init_data).unwrap();

		for segment in file.segments()
		                   .filter(|s| s.segment_type == SegmentType::LOAD) {
			assert_eq!(segment.alignment, 4096, "Not designed for !=1 page alignment");

			let addr = VirtualAddress::new(segment.vaddr.try_into().unwrap());
			let segment_page_offset = addr - *addr.align_down_to_page();

			let len = segment_page_offset + usize::try_from(segment.memory_size).unwrap();
			let len = len.div_ceil(4096);
			
			let (_, mut mapping) = mapping::Config::new(len.try_into().unwrap(), mapping::Ty::USER_CODE)
						.virtual_location(addr.align_down_to_page())
						.protection(
							segment.segment_flags.contains(SegmentFlags::Writeable),
							segment.segment_flags.contains(SegmentFlags::Executable),
							true,
						)
						.map_in::<mapping::UnsafeMmap>("/user/init.exec".into(), address_space)
						.unwrap();

			assert!(segment.file_size <= segment.memory_size);

			let base = unsafe {
				LocalUser::try_from(mapping.as_mut_ptr())
						.expect("`initd` should be loaded in loader thread")
						.byte_add(segment_page_offset)
			};
			debug_assert_eq!(base.addr(), addr, "mapping for elf executable should be same as addr in file");

			unsafe {
				// todo: replace with proper local pointer and permissions adjustments
				if is_x86_feature_detected!("smap") { core::arch::asm!("stac", options(nomem, nostack, preserves_flags)); }
				core::arch::asm!(
					"mov {0:r}, cr0",
                    "and {0:r}, ~0x10000",
					"mov cr0, {0:r}",
					out(reg) _,
					options(nomem, nostack, preserves_flags)
				);
				ptr::copy_nonoverlapping(
					file[segment.file_location()].as_ptr(),
					base.addr().as_ptr(),
					segment.file_size.try_into().unwrap(),
				);
				ptr::write_bytes(
					base.addr().as_ptr().byte_add(segment.file_size.try_into().unwrap()),
					0,
					(segment.memory_size - segment.file_size).try_into().unwrap(),
				);
				core::arch::asm!(
					"mov {0:r}, cr0",
					"or {0:r}, 0x10000",
					"mov cr0, {0:r}",
					out(reg) _,
					options(nomem, nostack, preserves_flags)
				);
				if is_x86_feature_detected!("smap") { core::arch::asm!("clac", options(nomem, nostack, preserves_flags)); }
			}
		}
		
		debug!("{address_space:?}");

		(VirtualAddress::new(file.entrypoint()), stack_top)
	};
	drop(init_data);
	{
		debug!("opening init stdin/out/err/thread/ramdisk as handles 0..=4");

		let _ = ipc::server::server_registry(); // force it to init builtin servers (root + proc)

		let guard = percpu::percpu_v2!(current_thread).read();
		let thread = guard.as_ref().unwrap();

		let stdio_handle = ipc::open(
			"console:/",
			&[<dyn ipc::protocol::generated::CoreIoRead>::UID, <dyn ipc::protocol::generated::CoreIoWrite>::UID],
			kernel_api::ptr::null(),
		).expect("unable to open console");
		let thread_handle = Handle::new(
			ipc::server::server_registry().get_server_at("proc").expect("unable to open `proc`").0,
			thread.thread_id.get() as isize,
			&[<dyn ipc::protocol::generated::CoreProcThread>::UID]
		);
		let ramdisk_handle = Handle::new(
			ramdisk_server,
			1,
			&[<dyn ipc::protocol::generated::CoreIoRead>::UID]
		);
		let acpi_handle = Handle::new(
			ramdisk_server,
			2,
			&[<dyn ipc::protocol::generated::CoreIoRead>::UID]
		);

		dbg!(&stdio_handle);
		dbg!(&thread_handle);
		dbg!(&acpi_handle);

		thread.handles.openat(0, stdio_handle.clone())
				.expect("unable to open fd 0");
		thread.handles.openat(1, stdio_handle.clone())
				.expect("unable to open fd 1");
		thread.handles.openat(2, stdio_handle)
				.expect("unable to open fd 2");
		thread.handles.openat(3, thread_handle)
				.expect("unable to open fd 3");
		thread.handles.openat(4, ramdisk_handle)
		      .expect("unable to open fd 4");
		thread.handles.openat(5, acpi_handle)
		      .expect("unable to open fd 5");
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
	if let Some(current_thread) = percpu_v2!(@current_thread)
			&& let Some(current_thread) = current_thread.try_read()
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
