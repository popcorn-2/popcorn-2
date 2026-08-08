#![feature(try_blocks)]
#![feature(arbitrary_self_types)]
#![feature(step_trait)]
#![feature(error_iter)]
#![feature(duration_integer_division)]
#![no_main]
#![no_std]

extern crate alloc;

use alloc::boxed::Box;
use core::convert::Infallible;
use core::error::Error;
use core::panic::PanicInfo;
use core::time::Duration;
use log::{debug, error, info};
use uefi::{entry, println, Status, boot};

//mod paging;
mod logging;
//mod elf;
mod fs;
mod loader;

const PAGE_MAP_OFFSET: u64 = 0xffff_8000_0000_0000;
const PAGE_MAP_OFFSET_LEN: u64 = 2u64.pow(46);

#[entry]
fn bootloader_entry() -> Status {
    let error = main().expect_err("Ok path will never return");
    error!("{error}");
    for source in error.sources() {
        error!(target: "<continuation>", "Caused by: {}", source);
    }

	debug!("{error:?}");

	boot::stall(Duration::from_secs(10));

    Status::ABORTED
}

fn main() -> Result<Infallible, Box<dyn Error>> {
    println!("Loading popcorn2...");

	logging::init()?;

	let mut rootfs = fs::locate_rootfs()?;
	let version = loader::find_latest_kernel(&mut *rootfs)?;
	info!("Booting kernel {version}");

	loop {}

	/*
    let (kernel, symbol_map, init_program, ramdisk) = locate_kernel(&image_handle, &services);

    let kernel = elf::load_kernel(kernel)?;
    let elf::KernelLoadInfo { kernel, mut page_table, address_range, kasan_enabled, stack_page_count } = kernel;
    let mut address_range = {
        address_range.start.align_down_to_page() .. address_range.end.align_up_to_page()
    };

    debug!("kernel placed at {address_range:#x?}");

    // map framebuffer
    let framebuffer_info: Option<handoff::Framebuffer> = try {
        use uefi::proto::console::gop::PixelBitmask;

        let mode_info = fb.2;
        let page_count = (fb.1 + PAGE_SIZE - 1) / PAGE_SIZE;
        address_range.start = address_range.start - page_count;
        let fb_start = address_range.start;

        let framebuffer_addr = fb.0 as usize;

        debug!("fb map {:#x} -> {:#x}", fb_start.addr, framebuffer_addr);

        page_table.try_map_range_with::<(), _>(
            Page(fb_start.addr.try_into().unwrap()),
            Frame(framebuffer_addr.try_into().unwrap()),
            page_count.try_into().unwrap(),
	        || {
                let addr = services.allocate_pages(AllocateType::AnyPages, MemoryType::LOADER_DATA, 1).map_err(|_| ())?;
                trace!("=== btl a {addr:#018x} -> {:#018x} : fb page tables", addr as usize + 4096);
                Ok(addr)
            },
	        TableEntryFlags::WRITABLE | TableEntryFlags::NO_EXECUTE | TableEntryFlags::MMIO,
	        Ty::FB,
        ).ok()?;

        let color_format = match mode_info.pixel_format() {
            PixelFormat::Rgb => Some(ColorMask::RGBX),
            PixelFormat::Bgr => Some(ColorMask::BGRX),
            PixelFormat::Bitmask => {
                let PixelBitmask{ red, green, blue, .. } = mode_info.pixel_bitmask().unwrap();
                Some(ColorMask{ red, green, blue })
            },
            PixelFormat::BltOnly => None
        }?;

        handoff::Framebuffer {
            buffer: fb_start.addr as *mut u8,
            stride: mode_info.stride(),
            width,
            height,
            color_format,
	        physical_address: PhysicalAddress::new(framebuffer_addr),
        }
    };

    let stack = {
        address_range.start = address_range.start - (STACK_PAGE_COUNT + 1); // `+ 1` for guard page

        let Ok(allocation) = services.allocate_pages(AllocateType::AnyPages, MemoryType::LOADER_DATA, STACK_PAGE_COUNT) else {
            panic!("Failed to allocate enough memory to load popcorn2");
        };
        trace!("=== btl a {allocation:#018x} -> {:#018x} : kernel stack", allocation as usize + STACK_PAGE_COUNT * 4096);

        debug!("stack map {:#x} -> {:#x}", address_range.start.addr+4096, allocation);

        page_table.try_map_range_with::<(), _>(Page((address_range.start.addr+4096).try_into().unwrap()), Frame(allocation), STACK_PAGE_COUNT.try_into().unwrap(), || {
            let addr = services.allocate_pages(AllocateType::AnyPages, MemoryType::LOADER_DATA, 1).map_err(|_| ())?;
            trace!("=== btl a {addr:#018x} -> {:#018x} : stack page table", addr as usize + 4096);
            Ok(addr)
        }, TableEntryFlags::NO_EXECUTE | TableEntryFlags::WRITABLE, Ty::KERNEL_STACK)?;

        handoff::Stack {
            bottom_virt: address_range.start,
            top_virt: address_range.start + STACK_PAGE_COUNT + 1usize,
            top_phys: RawFrame::new(allocation.try_into().unwrap()) + STACK_PAGE_COUNT,
        }
    };

		let mut allocate_shadow_memory = |page_table: &mut PageTable, start: RawPage, end: RawPage, val: u8| {
			use kernel_api::memory::asan;

			// fixme: this incorrectly unpoisons memory below and above the actual location
			let shadow_start = {
				let val = (start.addr >> 3) + asan::SHADOW_MAP_SHIFT;
				let aligned = VirtualAddress::new(val).align_down_to_page();
				Page(aligned.addr as u64)
			};
			let shadow_end = {
				let val = (end.addr >> 3) + asan::SHADOW_MAP_SHIFT;
				let aligned = VirtualAddress::new(val).align_up_to_page();
				Page(aligned.addr as u64)
			};
			debug!("map shadow memory for {shadow_start:x?} to {shadow_end:x?}");

			let page_count = (shadow_end.0 - shadow_start.0) / 4096;

		let Ok(allocation) = services.allocate_pages(AllocateType::AnyPages, MemoryType::LOADER_DATA, page_count as usize) else {
			panic!("Failed to allocate enough memory for shadow memory");
		};
		trace!("=== btl a {allocation:#018x} -> {:#018x} : shadow memory", allocation + 4096*page_count);

		let mut allocate = || -> Result<u64, ()> {
			let addr = services.allocate_pages(AllocateType::AnyPages, MemoryType::LOADER_DATA, 1).map_err(|_| ())?;
			trace!("=== btl a {addr:#018x} -> {:#018x} : shadow memory page table", addr as usize + 4096);
			Ok(addr)
		};

			for i in 0..page_count {
				let page = Page(shadow_start.0 + i * 4096);
				let frame = Frame(Frame(allocation).0 + i * 4096);

				match page_table.try_map_page_with(page, frame, &mut allocate, TableEntryFlags::NO_EXECUTE | TableEntryFlags::WRITABLE, Ty::SHADOW_MEM) {
					Ok(_) => {
						unsafe {
							core::ptr::write_bytes(
								frame.0 as *mut u8,
								val,
								4096,
							);
						}
					},
					Err(MapError::AlreadyMapped(Ty::SHADOW_MEM)) => {
						let translated = page_table.translate_page(page).expect("just got an error trying to map over this");
						let read = unsafe { (translated.0 as *mut u8).read() };
						if read != val { todo!("{read:#x} -> {val:#x} ({i})") }
						//services.free_pages(frame.0, 1).unwrap();
						//trace!(target: "allocsan", "=== btl d {:#018x} -> {:#018x}", frame.0, frame.0 + 4096);
						warn!("oopsie doopsie we're leaking memory ({:#018x})", frame.0);
					},
					e => return Err(e.into()),
				}
			}

			unsafe {
				core::ptr::write_bytes(
					allocation as *mut u8,
					val,
					(shadow_end.0 - shadow_start.0) as usize
				);
			}

			(shadow_start, allocation)
		};

    if kasan_enabled {
	    use kernel_api::memory::asan;

        // set up shadow memory for stack, framebuffer, and kernel executable
        let core::ops::Range { start, end } = address_range.clone();

	    let (true_start, allocation) = allocate_shadow_memory(&mut page_table, start, end, 0x00);

	    let stack_shadow_offset = ((stack.bottom_virt.addr >> 3) + asan::SHADOW_MAP_SHIFT) - (true_start.0 as usize);
	    debug!("set {} bytes of shadow to 0xf4 for stack guard page", 4096 / 8);
	    debug!("set {} bytes of shadow to 0 for stack memory", STACK_PAGE_COUNT * 4096 / 8);
	    unsafe {
		    let stack_guard_shadow = (allocation as *mut u8).byte_add(stack_shadow_offset);
		    let stack_shadow = stack_guard_shadow.byte_add(4096 / 8);
		    debug!("stack_shadow = {stack_guard_shadow:#p}");
		    core::ptr::write_bytes(
			    stack_guard_shadow,
			    0xf4,
			    4096 / 8
		    );
		    core::ptr::write_bytes(
			    stack_shadow,
			    0,
			    STACK_PAGE_COUNT * 4096 / 8
		    );
	    }

        // set up shadow mem for shadow memory
	    /*allocate_shadow_memory(
		    &mut page_table,
		    asan::SHADOW_MAP_START.align_down_to_page(),
		    asan::SHADOW_MAP_END.align_up_to_page(),
		    0xcc,
	    );*/
    }

    let symbol_map = symbol_map.map(|m| &*Box::leak(m.into_boxed_slice()));
    let init_program = &*Box::leak(init_program.into_boxed_slice());
    let ramdisk = &*Box::leak(ramdisk.into_boxed_slice());

    info!("new stack at {:#x?}", stack);

    let mut memory_map_buffer = {
        let size = services.memory_map_size();
        let size = size.map_size + size.entry_size * 16;
        vec![0u8; size]
    };

    // Allocate handoff upfront so it's in the memory map and gets mapped into kernel page tables
    let handoff = Box::leak(Box::<handoff::Data>::new_uninit());
    debug!("Allocated handoff structure at {handoff:#p} -> {:#p}", handoff.as_ptr().wrapping_offset(1));
    debug!(
        "Allocated memmap structure at {:#p} -> {:#p}",
        memory_map_buffer.as_ptr(),
        memory_map_buffer.as_ptr().wrapping_byte_offset(memory_map_buffer.len() as isize)
    );

	// Allocate mem map buffer upfront so it's in the memory map and gets mapped into kernel page tables
	let mut kernel_mem_map = {
		let size = services.memory_map_size();
		Vec::with_capacity(size.map_size / size.entry_size + 64) // add more capacity because we have a bunch of allocations after this
	};
    
    let memory_map = services.memory_map(
        MemoryDescriptor::align_buf(&mut memory_map_buffer).unwrap()
    )?;

    let stack_ptr: u64;
    unsafe { asm!("mov {}, rsp", out(reg) stack_ptr); }

	// create page map region
    for mem in memory_map.entries().filter(|mem|
            mem.ty == MemoryType::BOOT_SERVICES_CODE ||
            mem.ty == MemoryType::BOOT_SERVICES_DATA ||
            mem.ty == MemoryType::PERSISTENT_MEMORY ||
            mem.ty == MemoryType::LOADER_CODE ||
            mem.ty == MemoryType::LOADER_DATA ||
            mem.ty == MemoryType::ACPI_NON_VOLATILE ||
            mem.ty == MemoryType::ACPI_RECLAIM ||
            mem.ty == MemoryType::RUNTIME_SERVICES_CODE ||
            mem.ty == MemoryType::RUNTIME_SERVICES_DATA ||
            mem.ty == MemoryType::CONVENTIONAL
    ) {
        debug!("[page map] {:x?} ({:#x} -> {:#x}) -> {:#x}", mem.ty, mem.phys_start, mem.phys_start + mem.page_count * 4096, mem.phys_start + PAGE_MAP_OFFSET);

        // UEFI memory sections are always aligned by firmware
        (0..mem.page_count).map(|page_num| mem.phys_start + page_num * 4096).try_for_each(|addr| {
            let virt_addr = addr + PAGE_MAP_OFFSET;
            assert!(addr < PAGE_MAP_OFFSET_LEN, "Too much physical memory");
            page_table.try_map_page_with::<(), _>(Page(virt_addr), Frame(addr), || {
                let addr = services.allocate_pages(AllocateType::AnyPages, MemoryType::LOADER_DATA, 1).map_err(|_| ())?;
                trace!("=== btl a {addr:#018x} -> {:#018x} : page map page table", addr as usize + 4096);
                Ok(addr)
            }, TableEntryFlags::NO_EXECUTE | TableEntryFlags::WRITABLE, Ty::MEM_MAP)
        })?;

	    if kasan_enabled {
		    let virt_start = VirtualAddress::new((mem.phys_start + PAGE_MAP_OFFSET) as usize).align_up_to_page();
		    let virt_end = VirtualAddress::new((mem.phys_start + PAGE_MAP_OFFSET) as usize)
				    .add(mem.page_count as usize * 4096)
				    .align_down_to_page();

		    allocate_shadow_memory(
			    &mut page_table,
			    virt_start,
			    virt_end,
			    0x00,
		    )?;
	    }
    }

	// identity map bootloader data and bootloader stack
    for mem in memory_map.entries().filter(|mem|
            mem.ty == MemoryType::LOADER_DATA ||
                    (mem.phys_start..mem.phys_start + mem.page_count * 4096).contains(&stack_ptr)
    ) {
        debug!("{:x?} ({:#x} -> {:#x}) - {:?}", mem.ty, mem.phys_start, mem.phys_start + mem.page_count * 4096, mem.att);

        // UEFI memory sections are always aligned by firmware
        (0..mem.page_count).map(|page_num| mem.phys_start + page_num * 4096).try_for_each(|addr| {
            page_table.try_map_page_with::<(), _>(Page(addr), Frame(addr), || {
                let addr = services.allocate_pages(AllocateType::AnyPages, MemoryType::LOADER_DATA, 1).map_err(|_| ())?;
                trace!("=== btl a {addr:#018x} -> {:#018x} : bootloader data page table", addr as usize + 4096);
                Ok(addr)
            }, TableEntryFlags::NO_EXECUTE | TableEntryFlags::WRITABLE, Ty::LOADER_DATA)
        })?;
    }

	// identity map bootloader code
    for mem in memory_map.entries().filter(|mem| mem.ty == MemoryType::LOADER_CODE) {
        debug!("{:x?} ({:#x} -> {:#x}) - {:?}", mem.ty, mem.phys_start, mem.phys_start + mem.page_count * 4096, mem.att);

        // UEFI memory sections are always aligned by firmware
        (0..mem.page_count).map(|page_num| mem.phys_start + page_num * 4096).try_for_each(|addr| {
            page_table.try_map_page_with::<(), _>(Page(addr), Frame(addr), || {
                let addr = services.allocate_pages(AllocateType::AnyPages, MemoryType::LOADER_DATA, 1).map_err(|_| ())?;
                trace!("=== btl a {addr:#018x} -> {:#018x} : bootloader code page table", addr as usize + 4096);
                Ok(addr)
            }, TableEntryFlags::WRITABLE, Ty::LOADER_CODE)
        })?;
    }

	// since the allocations above will have affected the memory map, regenerate it before converting to kernel handoff format
	{
		let size = services.memory_map_size();
		let size = size.map_size + size.entry_size * 16;
		memory_map_buffer.reserve(
			size.saturating_sub(memory_map_buffer.capacity())
		);
	}

	let mut memory_map = services.memory_map(
		MemoryDescriptor::align_buf(&mut memory_map_buffer).unwrap()
	)?;

    debug!("Generating kernel memory map");
    let kernel_mem_map = {
        let descriptor_to_entry = |descriptor: &MemoryDescriptor| {
            use handoff::MemoryType::*;

            let ty = match descriptor.ty {
                MemoryType::CONVENTIONAL |
                MemoryType::BOOT_SERVICES_CODE |
                MemoryType::BOOT_SERVICES_DATA |
                MemoryType::PERSISTENT_MEMORY => Free,
                MemoryType::LOADER_CODE => BootloaderCode,
                MemoryType::LOADER_DATA => BootloaderData,
                MemoryType::ACPI_NON_VOLATILE => AcpiPreserve,
                MemoryType::ACPI_RECLAIM => AcpiReclaim,
                MemoryType::RUNTIME_SERVICES_CODE => RuntimeCode,
                MemoryType::RUNTIME_SERVICES_DATA => RuntimeData,
                _ => Reserved
            };

            MemoryMapEntry {
                coverage: Range(PhysicalAddress::new(descriptor.phys_start.try_into().unwrap()), PhysicalAddress::new((descriptor.phys_start + descriptor.page_count * 4096).try_into().unwrap())),
                ty
            }
        };

        memory_map.sort();
        let mem_map_data = memory_map.entries().map(|entry| {
            descriptor_to_entry(entry)
        });
        assert_lt!(mem_map_data.len(), kernel_mem_map.capacity());
        let last_item = mem_map_data.reduce(|old_item, new_item| {
            if old_item.ty == new_item.ty && old_item.coverage.1 == new_item.coverage.0 {
                MemoryMapEntry {
                    ty: old_item.ty,
                    coverage: Range(old_item.coverage.0, new_item.coverage.1)
                }
            } else {
                kernel_mem_map.push(old_item);
                new_item
            }
        });
        if let Some(item) = last_item {
            kernel_mem_map.push(item);
        };
        Vec::leak(kernel_mem_map)
    };

    let kernel_entry = kernel.entry_point();
    debug!("Handover to kernel with entrypoint at {:#x}", kernel_entry);

    drop(gop);
    drop(fs);
    drop(uart);
    drop(mouse);
    drop(keyboard);

    let config_tables = system_table.config_table();
    let rsdp = if let Some(xsdp) = config_tables.iter().find(|table| table.guid == cfg::ACPI2_GUID) {
        PhysicalAddress::new(xsdp.address as usize)
    } else if let Some(rsdp) = config_tables.iter().find(|table| table.guid == cfg::ACPI_GUID) {
        PhysicalAddress::new(rsdp.address as usize)
    } else {
        panic!("No RSDP found");
    };

    let handoff = handoff.write(handoff::Data {
        framebuffer: framebuffer_info,
        memory: handoff::Memory {
            map: kernel_mem_map,
            used: Range(address_range.start, address_range.end),
            stack,
        },
        modules: handoff::Modules {

        },
        log: handoff::Logging {
            symbol_map: symbol_map.map(NonNull::from),
        },
        test: handoff::Testing {
            module_func: unsafe { mem::transmute(1usize) }
        },
        rsdp,
        init_exec: init_program,
        ramdisk,
    });

	trace!("{handoff:x?}");

    let mut memory_map = unsafe { boot::exit_boot_services(None) };
	memory_map.sort();


    unsafe {
        // Enable write-protect bit
        asm!(
            "mov {0:r}, cr0",
            "or {0:r}, 0x10000",
            "mov cr0, {0:r}",
            out(reg) _
        );
        // Enable NX enable and syscall/sysret bits
        asm!(
            "rdmsr",
            "or eax, 0x801",
            "wrmsr",
            in("ecx") 0xC0000080u32,
            out("eax") _,
            out("edx") _
        );
    }

    page_table.switch();

    unsafe {
        asm!(
            "mov rsp, rcx",
            "xor ebp, ebp",
            "push 0",

            "mov eax, 0xead10ca1",
            "mov edx, 0xd", // edx:eax = 0xdead10ca1 ('dead local')
            "mov ecx, 0xc0000101", // ecx = GSBase MSR
            "wrmsr",

            "jmp rsi",
        in("rcx") stack.top_virt.addr, in("rsi") kernel_entry, in("rdi") handoff, options(noreturn))
    }*/
}

#[panic_handler]
fn panic_handler(info: &PanicInfo<'_>) -> ! {
    error!("{info}");

	loop {} // watchdog timer should kill us after long enough
}

/*
| UEFI type                  | Use                                                              |
|----------------------------|------------------------------------------------------------------|
| EfiReservedMemoryType      | Unusable memory                                                  |
| EfiLoaderCode              | Bootloader code                                                  |
| EfiLoaderData              | Bootloader data and memory allocations by bootloader             |
| EfiBootServicesCode        | Boot services driver code - preserve to use boot services        |
| EfiBootServicesData        | Boot services driver data - preserve to use boot services        |
| EfiRuntimeServicesCode     | Runtime services driver code - preserve to use runtime services |
| EfiRuntimeServicesData     | Runtime services driver data - preserve to use runtime services |
| EfiConventionalMemory      | Free memory                                                      |
| EfiUnusableMemory          | Memory with errors detected                                      |
| EfiACPIReclaimMemory       | Memory containing ACPI tables - preserve until parsing ACPI      |
| EfiACPIMemoryNVS           | ACPI firmware memory that must be preserved across sleep         |
| EfiMemoryMappedIO          | ???                                                              |
| EfiMemoryMappedIOPortSpace | ???                                                              |
| EfiPalCode                 | ???                                                              |
| EfiPersistentMemory        | Non-volatile but otherwise conventional memory                   |
| EfiUnacceptedMemoryType    | ???                                                              |
 */

mod paging_reasons {
	use elf::segment::{Flags, Type};
	use kernel_api::mapping::Ty;

	pub fn kernel_seg_to_mapping_ty(ty: Type, flags: Flags) -> Ty {
		match ty {
            Type::LOAD if flags.contains(Flags::Executable) => Ty::KERNEL_CODE,
            Type::LOAD => Ty::KERNEL_DATA,
            Type::TLS => Ty::KERNEL_TLS,
			_ => Ty::KERNEL_OTHER,
		}
	}
}
