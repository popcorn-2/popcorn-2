use core::arch::global_asm;

macro_rules! export {
    ($export_name:ident <=> $aliased_fn:path) => {
        ::core::arch::global_asm!(
            concat!(".global ", stringify!($export_name)),
            concat!(".set ", stringify!($export_name), ", {}"),
            sym $aliased_fn,
        );
    };
}

// functions
export!(__popcorn_async_spawn_task <=> crate::ipc::executor::spawn);
#[cfg(not(feature = "hal-next"))] export!(__popcorn_disable_irq <=> crate::hal::get_and_disable_interrupts);
#[cfg(feature = "hal-next")] export!(__popcorn_disable_irq <=> crate::arch::get_and_disable_interrupts);
export!(__popcorn_ebr_defer_and_clean <=> crate::ebr::defer_and_cleanup);
export!(__popcorn_enable_irq <=> crate::hal::enable_interrupts);
export!(__popcorn_ksyscall_blocking <=> crate::ipc::kernel_syscall_blocking);
export!(__popcorn_print_stack_trace <=> crate::panicking::stack_trace);
export!(__popcorn_set_irq <=> crate::hal::set_interrupts);
export!(__popcorn_stack_trace_iter <=> crate::panicking::stack_trace_fn);
export!(__popcorn_system_time <=> crate::timing::tsc);
export!(__popcorn_system_time_scale <=> crate::timing::tsc_to_nanos);

// globals
export!(__popcorn_memory_physical_dmamem <=> crate::memory::physical::GLOBAL_DMA);
export!(__popcorn_memory_physical_highmem <=> crate::memory::physical::GLOBAL_HIGHMEM);
export!(__popcorn_memory_virtual_kernel_global <=> crate::memory::r#virtual::GLOBAL_VIRTUAL_ALLOCATOR);

// KASAN state
global_asm!(
    r#".section ".note.popcorn.kasan", "a", @note"#,
    ".balign 4",
    ".long 8",
    ".long 1",
    ".long 0x200", // kasan state
    r#".asciz "Popcorn""#,
    ".balign 4",
    #[cfg(kasan)] ".byte 1",
    #[cfg(not(kasan))] ".byte 0",
    ".balign 4",
);

// Initial stack size
global_asm!(
    r#".section ".note.popcorn.stack", "a", @note"#,
    ".balign 4",
    ".long 8",
    ".long 4",
    ".long 0x201", // stack page count
    r#".asciz "Popcorn""#,
    ".balign 4",
    #[cfg(any(kasan, debug_assertions))] ".long 136",
    #[cfg(not(any(kasan, debug_assertions)))] ".long 34",
    ".balign 4",
);
