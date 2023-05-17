ENTRY(_start)

SECTIONS {
    . = 0x8000;
    .ap_bootstrap.text ALIGN(4K) :
    {
        *ap_bootstrap*(.text*)
    }

    .ap_bootstrap.data ALIGN(4K) :
    {
        KEEP(*(.multiboot2*))
        *ap_bootstrap*(.data*)
    }

	KERNEL_OFFSET = 0xFFFFFFFF80000000;
	. = 1M + KERNEL_OFFSET;

	.kernelspace.text ALIGN(4K) : AT(ADDR(.kernelspace.text) - KERNEL_OFFSET)
	{
		*(EXCLUDE_FILE(*userspace* *ap_bootstrap*) .text*)
	}
 
	/* Read-only data. */
	.kernelspace.rodata ALIGN(4K) : AT(ADDR(.kernelspace.rodata) - KERNEL_OFFSET)
	{
		. = ALIGN(64);
		preinit_array_start = .;
		*(SORT(.preinit_array*))
		preinit_array_end = .;

		. = ALIGN(64);
		init_array_start = .;
		*(SORT(.init_array*))
		init_array_end = .;

		. = ALIGN(64);
		start_ctors = .;
        *(SORT(.ctors*))
        end_ctors = .;

		. = ALIGN(64);
        start_dtors = .;
        *(SORT(.dtors*))
        end_dtors = .;

		*(EXCLUDE_FILE(*userspace* *ap_bootstrap*) .rodata*)

		font_psf_start = .;
		KEEP(*(.font.rodata))
		font_psf_end = .;
	}

    .eh_frame ALIGN(4K) : AT(ADDR(.eh_frame) - KERNEL_OFFSET) {
        __eh_frame_start = .;
        KEEP(*(.eh_frame))
        __eh_frame_end = .;
    }

    .eh_frame_hdr ALIGN(4K) : AT(ADDR(.eh_frame_hdr) - KERNEL_OFFSET) {
       KEEP(*(.eh_frame_hdr))
    }

    __eh_frame_hdr_start = SIZEOF(.eh_frame_hdr) > 0 ? ADDR(.eh_frame_hdr) : 0;
    __eh_frame_hdr_end = SIZEOF(.eh_frame_hdr) > 0 ? . : 0;

    .gcc_except_table ALIGN(4K) : AT(ADDR(.gcc_except_table) - KERNEL_OFFSET) {
       KEEP(*(.gcc_except_table*))
    }
 
	/* Read-write data (initialized) */
	.kernelspace.data ALIGN(4K) : AT(ADDR(.kernelspace.data) - KERNEL_OFFSET)
	{
		*(EXCLUDE_FILE(*userspace* *ap_bootstrap*) .data*)
		*(EXCLUDE_FILE(*userspace* *ap_bootstrap*) .got*)
	}
 
	/* Read-write data (uninitialized) and stack */
	.kernelspace.bss ALIGN(4K) : AT(ADDR(.kernelspace.bss) - KERNEL_OFFSET)
	{
		*(COMMON)
		*(EXCLUDE_FILE(*userspace* *ap_bootstrap*) .bss*)
	}

	.kernelspace.tbss ALIGN(4K) : AT(ADDR(.kernelspace.tbss) - KERNEL_OFFSET)
    {
    		*(EXCLUDE_FILE(*userspace* *ap_bootstrap*) .tbss*)
    }

	.userspace.text ALIGN(4K) : AT(ADDR(.userspace.text) - KERNEL_OFFSET)
	{
	    *userspace*(.text*)
	}

	.userspace.rodata ALIGN(4K) : AT(ADDR(.userspace.rodata) - KERNEL_OFFSET)
    {
        *userspace*(.rodata*)
    }

	.userspace.data ALIGN(4K) : AT(ADDR(.userspace.data) - KERNEL_OFFSET)
    {
        *userspace*(.data*)
    }

    .userspace.bss ALIGN(4K) : AT(ADDR(.userspace.bss) - KERNEL_OFFSET)
    {
        *userspace*(COMMON)
        *userspace*(.bss*)
    }

	/DISCARD/ : {
		*(.debug_gdb_scripts*)
	}
}
