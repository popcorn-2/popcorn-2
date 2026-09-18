// rust features
#![feature(custom_test_frameworks)]
#![test_runner(test_harness::test_runner)]
#![reexport_test_harness_main = "test_main"]
#![feature(const_trait_impl)]
#![feature(allocator_api)]
#![feature(ptr_metadata)]
#![feature(sync_unsafe_cell)]
#![feature(arbitrary_self_types)]
#![feature(min_specialization)]
#![feature(doc_cfg)]
#![feature(integer_atomics)]
#![feature(arbitrary_self_types_pointers)]
#![feature(linkage)]
#![feature(prelude_import)]
#![feature(derive_const)]
#![feature(const_default)]
#![feature(const_convert)]
#![feature(abi_custom)]
#![feature(integer_widen_truncate)]
#![feature(rust_preserve_none_cc)]
#![feature(decl_macro)]
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
use core::ptr::slice_from_raw_parts_mut;
use kernel_api::memory::{PhysicalAddress, RawFrame, RawPage, VirtualAddress};
use core::cmp::{max, min};
#[cfg(kasan)] use core::marker::PhantomData;
use core::time::Duration;
use utils::handoff::MemoryType;
use utils::handoff::MemoryMapEntry;
#[cfg(kasan)] use crate::hal::paging2::Flags;
use crate::hal::paging2::TTable;
use crate::memory::watermark_allocator::WatermarkAllocator;
use crate::task::Task;

mod abi;
mod arch;
mod ebr;
mod hal;
mod ipc;
mod logging;
mod memory;
mod panicking;
mod percpu;
pub use percpu::percpu;
mod prelude;
mod syscall;
mod task;
mod timing;

// The compiler expects the prelude definition to be defined before it's use statement
mod private {
	#[prelude_import]
	#[allow(unused_imports)]
	pub use super::prelude::*;
}

#[cfg(test)]
pub mod test_harness;

fn get_foo() -> usize {
	unsafe { *percpu!(foo).get() }
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

mod handoff {
	use core::marker::PhantomData;
	use core::ptr::NonNull;
	use kernel_api::memory::asan::no_asan_shim;
	use utils::handoff::MemoryMapEntry;
	use crate::panicking;
	use crate::panicking::SymbolMap;
	use core::ops::Range;
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
		pub init_utable: (TTableTy, RawPage),
		pub init_entry: VirtualAddress,
	}

	pub fn process_handoff(handoff_data: &*const utils::handoff::Data) -> ParsedHandoff<'_> {
		debug!("parsing handoff data");
		let (s_table, u_table_bootloader, u_table_pid0) = no_asan_shim!(|handoff_data: &*const utils::handoff::Data| -> (RawFrame, RawFrame, (RawFrame, RawPage)) {
			unsafe { (
				(**handoff_data).memory.s_table,
				(**handoff_data).memory.u_table_bootloader,
				(**handoff_data).memory.u_table_pid0,
			) }
		});

		let s_table = unsafe { KTableTy::from_raw(s_table) };
		unsafe { crate::memory::paging::init_page_table(s_table) };

		let bootloader_utable = unsafe { TTableTy::from_raw(u_table_bootloader) };
		let u_table_pid0 = (unsafe { TTableTy::from_raw(u_table_pid0.0) }, u_table_pid0.1);

		let symbol_map = no_asan_shim!(|handoff_data: &*const utils::handoff::Data| -> Option<NonNull<[u8]>> {
			unsafe { (**handoff_data).log.symbol_map }
		});
		*panicking::SYMBOL_MAP.write() = SymbolMap::from(symbol_map);

		no_asan_shim!(|handoff_data: &*const utils::handoff::Data, bootloader_utable: TTableTy, u_table_pid0: (TTableTy, RawPage)| -> ParsedHandoff<'_> {
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
				init_utable: u_table_pid0,
				init_entry,
			}
		})
	}
}

fn init(handoff_data: *const utils::handoff::Data) -> (VirtualAddress, usize) {
	sprintln!("POP");

	let _ = logging::init();

	percpu::Percpu::bsp_init();

	let x = get_foo();
	assert_eq!(x, 6, "TLS value should be 6");

	arch::target_bsp_start();

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

		let allocator = memory::physical::with_highmem_as(&watermark_allocator, || unsafe {
			bitmap_allocator::BitmapAllocator::new(
				RawFrame::new(0)..max_usable_memory.align_down_to_frame(),
				all_spaces,
			).expect("unable to create highmem")
		});

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

	arch::post_memory_init(parsed_handoff.rsdp);

	let (_update_line, _picos_per_tick) = {
		let fb = parsed_handoff.framebuffer;
		let size = fb.stride * fb.height;
		let stride = fb.stride;
		let fb_data = unsafe { &mut *slice_from_raw_parts_mut(fb.buffer.cast::<u32>(), size) };

		// Clear to true black to match background of BGRT logo
		for pixel in fb_data.iter_mut() {
			*pixel = 0;
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

	// all bootloader data structures parsed so safe to switch to PID0 utable and drop bootloader
	// SAFETY: `init_utable` ownership transferred into PID0 TCB
	unsafe { parsed_handoff.init_utable.0.load() };
	drop(parsed_handoff.bootloader_utable);

	percpu!(arch).tss.set_rsp0(*(parsed_handoff.stack.bottom_virt + parsed_handoff.stack.page_count));

	let ebr = percpu!(epoch).pin();
	let init_task = task::init(parsed_handoff.init_utable);
	let init_arg = task::ProcInfo::new_in(
		init_task,
		&["init", "--foo", "--bar"],
		vec![],
		0,
		VirtualAddress::new(0),
		&ebr,
	).expect("failed to set up startup info for pid0");

	(parsed_handoff.init_entry, init_arg.addr)
}

#[unsafe(export_name = "_start")]
extern "sysv64" fn kmain(handoff_data: *const utils::handoff::Data) -> ! {
	let (init_entry, init_arg) = init(handoff_data);

	arch::switch_to_userspace(init_entry, init_arg);
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
