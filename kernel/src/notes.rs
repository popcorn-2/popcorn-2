use core::arch::global_asm;

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
