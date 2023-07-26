%include "constants.inc"

bits 32

section .rodata

global lm_enabled
global hello_world_str
hello_world_str db "Hello World!", 0
color db 0x0f

%define col_reset 128
%define col_red 129
%define col_green 130

nl db 0xa, 0

cpuid_check db "[    ] Checking for CPUID support", 0xa, 0
cpuid_found db "[", col_green, " OK ", col_reset, "] CPUID supported", 0xa, 0
cpuid_not_found db "[", col_red, "FAIL", col_reset, "] CPUID support not found", 0xa, 0

cpuid_max_check db "[    ] Checking for extended CPUID support", 0xa, 0
cpuid_max_found db "[", col_green, " OK ", col_reset, "] Extended CPUID supported. Max feature: 0x", 0
cpuid_max_not_found db "[", col_red, "FAIL", col_reset, "] Extended CPUID support not found", 0xa, 0

long_mode_check db "[    ] Checking for long mode support", 0xa, 0
long_mode_found db "[", col_green, " OK ", col_reset, "] Long mode supported", 0xa, 0
long_mode_not_found db "[", col_red, "FAIL", col_reset, "] Long mode not supported", 0xa, 0

init_paging db "[    ] Initializing paging", 0xa, 0
init_paging_done db "[", col_green, " OK ", col_reset, "] Paging initialized", 0xa, 0
enable_paging_done db "[", col_green, " OK ", col_reset, "] Paging enabled", 0xa, 0

lm_enabling db "[    ] Entering long mode", 0xa, 0
lm_gdt_load db "[", col_green, " OK ", col_reset, "] Loaded GDT", 0xa, 0

gdt64:
	.null:
		dq 0 ; null entry
	.code_kernel:
		dd 0xFFFF ; limit low
		db 0 ; base low
		db .PRESENT | .S_BIT | .EXEC | .RW ; present, S bit, code, read/write, ring 0
		db .GRANULARITY | .LONG_MODE | 0xF ; granular, long mode, limit high
		db 0 ; base high
	.data_kernel:
		dd 0xFFFF ; limit low
		db 0 ; base low
		db .PRESENT | .S_BIT | .RW ; present, S bit, data, read/write, ring 0
		db .GRANULARITY | .S_BIT | 0xF ; granular, long mode, limit high
		db 0 ; base high
	.pointer:
		dw $ - gdt64 - 1 - KERNEL_OFFSET
		dq gdt64 - KERNEL_OFFSET

	.PRESENT		equ 1 << 7
	.RING_3			equ 3 << 5
	.S_BIT			equ 1 << 4
	.EXEC			equ 1 << 3
	.DC				equ 1 << 2
	.RW				equ 1 << 1
	.ACCESSED		equ 1 << 0

	.GRANULARITY	equ 1 << 7
	.SZ_32			equ 1 << 6
	.LONG_MODE		equ 1 << 5

section .text
global _start
extern _32_puts
extern _32_utoa
extern _32_print_cpu_info
extern long_mode_start

extern read_multiboot_info

extern framebuffer_addr
extern framebuffer_pitch
extern framebuffer_width
extern framebuffer_height
extern framebuffer_bpp
extern framebuffer_type

extern psf_copychar
extern parse_psf
extern _32_init_io

extern initial_mem_map_start

_start:
	mov esp, _stack_top - KERNEL_OFFSET
	mov ebp, esp
	push eax ; save multiboot magic
	push ebx ; save multiboot info
	
	push ebx ; pass multiboot info
	call read_multiboot_info
	add esp, 4 ; clear args

	; IN CASE BOOTED TO TEXT TERMINAL
	mov eax, [framebuffer_addr - KERNEL_OFFSET]
	cmp eax, 0xb8000
	jne .1
		mov eax, 0x0C450C54 ; TE
		mov [0xb8000], eax
		mov eax, 0x0C540C58 ; XT
		mov [0xb8004], eax
		mov eax, 0x0C4D0C20 ;  M
		mov [0xb8008], eax
		mov eax, 0x0C440C4F ; OD
		mov [0xb800c], eax
		mov eax, 0x0C200C45 ; E 
		mov [0xb8010], eax
		mov eax, 0x0C4F0C4E ; NO
		mov [0xb8014], eax
		mov ax, 0x0C54 ; T
		mov [0xb8018], ax
		mov eax, 0x0C530C20 ;  S
		mov [0xb801a], eax
		mov eax, 0x0C500C55 ; UP
		mov [0xb801e], eax
		mov eax, 0x0C4F0C50 ; PO
		mov [0xb8022], eax
		mov eax, 0x0C540C52 ; RT
		mov [0xb8026], eax
		mov eax, 0x0C440C45 ; ED
		mov [0xb802a], eax
		jmp error
.1:
	call parse_psf
	
	call _32_init_io
	
	; CHECK CPUID SUPPORTED
	;push cpuid_check ; print checking for cpuid
	mov eax, cpuid_check - KERNEL_OFFSET
	push eax
	call _32_puts
	add esp, 4 ; clear args
	
	pushfd
	pop eax ; move flags into eax
	mov ebx, eax ; save old flags
	xor eax, 1<<21 ; flip bit 21
	or eax, 3<<12 ; set iopl to ring 3
	push eax
	popfd ; write back flags

	pushfd
	pop eax ; move modified flags into eax
	push ebx
	popfd ; restore old flags

	xor eax, ebx
	jnz .2
		mov eax, cpuid_not_found - KERNEL_OFFSET
		push eax
		call _32_puts
		add esp, 4 ; clear args
		jmp error
	.2: 
		mov eax, cpuid_found - KERNEL_OFFSET
		push eax
		call _32_puts
		add esp, 4 ; clear args
	

	; CHECK MAX CPUID EXTENDED FEATURESET SUPPORTED
	mov eax, cpuid_max_check - KERNEL_OFFSET
	push eax
	call _32_puts
	add esp, 4 ; clear args

	mov eax, 0x80000000
	cpuid
	push eax ; save max extended feature set

	cmp eax, 0x80000001
	jnb .3
		mov eax, cpuid_max_not_found - KERNEL_OFFSET
		push eax
		call _32_puts
		add esp, 4 ; clear args
		jmp error
	.3: 
		mov eax, cpuid_max_found - KERNEL_OFFSET
		push eax
		call _32_puts
		add esp, 4 ; clear args

		pop eax ; get max featureset
		push 16 ; print in base 16
		push eax ; push max featureset
		call _32_utoa
		add esp, 8 ; clear args

		push eax ; print result of itoa
		call _32_puts
		add esp, 4 ; clear args

		mov eax, nl - KERNEL_OFFSET
		push eax ; print newline
		call _32_puts
		add esp, 4 ; clear args
	

	; CHECK LONG MODE SUPPORTED
	mov eax, long_mode_check - KERNEL_OFFSET
	push eax
	call _32_puts
	add esp, 4 ; clear args

	mov eax, 0x80000001
	cpuid
	test edx, 1<<29 ; test LM bit
	jnz .4
		mov eax, long_mode_not_found - KERNEL_OFFSET
		push eax
		call _32_puts
		add esp, 4 ; clear args
		jmp error
	.4:
		mov eax, long_mode_found - KERNEL_OFFSET
		push eax
		call _32_puts
		add esp, 4 ; clear args

	; INIT PAGING
	mov eax, init_paging - KERNEL_OFFSET
	push eax
	call _32_puts
	add esp, 4 ; clear args

    ; Identity mapping to keep running
	mov eax, level3_low_page_table - KERNEL_OFFSET
	or eax, 0b11 ; set present and write bits
	mov [level4_page_table + 8*0 - KERNEL_OFFSET], eax ; map p4[0] to p3_low

    ; Mapping to mem_map_start
	mov eax, level3_mem_map - KERNEL_OFFSET
    or eax, 0b11 ; set present and write bits
    mov [level4_page_table + 8*256 - KERNEL_OFFSET], eax ; map p4[256] to p3_mem_map

	; Mapping to page_offset_start
	mov eax, level3_low_page_table - KERNEL_OFFSET
    or eax, 0b11 ; set present and write bits
    mov [level4_page_table + 8*288 - KERNEL_OFFSET], eax ; map p4[288] to p3_low

    ; Higher half mapping
	mov eax, level3_high_page_table - KERNEL_OFFSET
    or eax, 0b11 ; set present and write bits
    mov [level4_page_table + 8*511 - KERNEL_OFFSET], eax ; map p4[511] to p3_high

	mov eax, level2_page_table_kmap - KERNEL_OFFSET
    or eax, 0b11 ; set present and write bits
    mov [level3_low_page_table + 8*0 - KERNEL_OFFSET], eax ; map p3_low[0] to p2_kmap

    mov eax, level2_page_table_fbmap - KERNEL_OFFSET
    or eax, 0b11 ; set present and write bits
    mov [level3_low_page_table + 8*1 - KERNEL_OFFSET], eax ; map p3_low[1] to p2_fbmap

	mov eax, level2_page_table_kmap - KERNEL_OFFSET
	or eax, 0b11 ; set present and write bits
	mov [level3_high_page_table + 8*510 - KERNEL_OFFSET], eax ; map p3_high[510] to p2_kmap

	mov eax, level2_page_table_fbmap - KERNEL_OFFSET
	or eax, 0b11 ; set present and write bits
	mov [level3_high_page_table + 8*511 - KERNEL_OFFSET], eax ; map p3_high[511] to p2_fbmap

    ; Mapping to mem_map_start
    mov eax, level2_mem_map - KERNEL_OFFSET
    or eax, 0b11 ; set present and write bits
    mov [level3_mem_map + 8*0 - KERNEL_OFFSET], eax ; map p3_mem_map[0] to p2_mem_map

    ; map p2_mem_map[0->2] to a huge page (2MiB)
    mov ecx, 0
    .map_level2_page_table_mem_map_loop:
        mov eax, 0x200000 ; size of each entry (2MiB)
        mul ecx ; real start address of entry (counter * 2MiB)
        lea eax, [initial_mem_map_start - KERNEL_OFFSET] ; start mapping from initial_mem_map region
        or eax, 0b10000011 ; present, write, and huge bits
        mov [level2_mem_map + ecx * 8 - KERNEL_OFFSET], eax ; map p2[counter] to eax

        inc ecx
        cmp ecx, 2
    jne .map_level2_page_table_mem_map_loop

	; map each p2_0 entry to a huge page (2MiB)
	mov ecx, 0
	.map_level2_page_table_kmap_loop:
		mov eax, 0x200000 ; size of each entry (2MiB)
		mul ecx ; real start address of entry (counter * 2MiB)
		or eax, 0b10000011 ; present, write, and huge bits
		mov [level2_page_table_kmap + ecx * 8 - KERNEL_OFFSET], eax ; map p2[counter] to eax

		inc ecx
		cmp ecx, 512 ; 512 entries in p2
	jne .map_level2_page_table_kmap_loop

	; map each p2_1 entry to a huge page (2MiB)
	mov ecx, 0
	.map_level2_page_table_fbmap_loop:
		mov eax, 0x200000 ; size of each entry (2MiB)
		mul ecx ; real start address of entry (counter * 2MiB)
		add eax, [framebuffer_addr - KERNEL_OFFSET] ; start mapping from 0xfd000000
		or eax, 0b10000011 ; present, write, and huge bits
		mov [level2_page_table_fbmap + ecx * 8 - KERNEL_OFFSET], eax ; map p2[counter] to eax

		inc ecx
		cmp ecx, 16 ; only map 4K 32bpp framebuffer
	jne .map_level2_page_table_fbmap_loop

	mov eax, init_paging_done - KERNEL_OFFSET
	push eax
	call _32_puts
	add esp, 4 ; clear args

	; enable paging
	mov eax, level4_page_table - KERNEL_OFFSET
	mov cr3, eax ; set p4 as l4 page table

	; enable PAE
	mov eax, cr4
	or eax, 1<<5 ; set PAE bit
	mov cr4, eax

	mov eax, enable_paging_done - KERNEL_OFFSET
	push eax
	call _32_puts
	add esp, 4 ; clear args

	; ENTER LONG MODE
	mov eax, lm_enabling - KERNEL_OFFSET
	push eax
	call _32_puts
	add esp, 4 ; clear args

	mov ecx, 0xC0000080 ; EFER rregister
	rdmsr
	or eax, 1<<0 ; set syscall/sysret bit
	or eax, 1<<8 ; set long mode bit
	or eax, 1<<11 ; set NXE bit
	wrmsr

	mov eax, cr0 ; enable paging
	or eax, 1<<16 ; set write protect bit
	or eax, 1<<31 ; set paging bit
	mov cr0, eax

	; update framebuffer_addr now that paging is enabled
	mov dword [framebuffer_addr - KERNEL_OFFSET], 0x40000000
	; load gdt
	lgdt [gdt64.pointer - KERNEL_OFFSET]
	mov eax, lm_gdt_load - KERNEL_OFFSET
	push eax
	call _32_puts
	add esp, 4 ; clear args
	jmp 0x8:long_mode_start - KERNEL_OFFSET
error:
loop: hlt
	jmp loop

extern level4_page_table

section .bss
align 4096
global _stack_top
level4_page_table:
	resb 0x1000
level3_high_page_table:
	resb 0x1000
level3_low_page_table:
	resb 0x1000
level3_mem_map:
    resb 0x1000
level2_page_table_kmap:
	resb 0x1000
level2_page_table_fbmap:
	resb 0x1000
level2_mem_map:
	resb 0x1000
_stack_bottom:
	resb 0x1000*4
_stack_top: