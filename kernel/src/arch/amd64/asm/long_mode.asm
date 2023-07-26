bits 64

global long_mode_start

%define col_reset 0x1b, "[0m"
%define col_red 0x1b, "[31m"
%define col_green 0x1b, "[32m"

section .rodata
sse_enabling db "[    ] Enabling SSE support", 0xa, 0
sse_enabled db "[ ", col_green, "OK", col_reset, " ] SSE enabled", 0xa, 0

section .text
extern kmain
extern puts
extern _stack_top
extern gdt64.pointer64
extern level4_page_table

long_mode_start:
	; switch data segment registers
	mov ax, 0x10
	mov ss, ax
	mov ax, 0
	mov ds, ax
	mov es, ax
	mov fs, ax
	mov gs, ax

	; jump to higher half
	mov rax, .1
	jmp rax
.1:
	; ENABLE SSE
	lea rdi, [rel sse_enabling]
	call puts

	xor eax, eax
	mov [level4_page_table + 8*0], eax ; unmap p4[0]

	mov rax, cr0
	and ax, ~(1<<2) ; clear CR0.EM
	or ax, 1<<1 ; enable coprocessor monitoring
	mov cr0, rax

	mov rax, cr4
	or ax, 1<<9 ; enable SSE instructions
	or ax, 1<<10 ; enable SSE exceptions
	mov cr4, rax

	lea rdi, [rel sse_enabled]
	call puts

	mov esi, [rbp-8] ; pop multiboot info
	mov edi, [rbp-4] ; pop multiboot magic
	mov rsp, _stack_top ; reset stack pointer to stack top

	xor rbp, rbp ; null base pointer
	jmp kmain
	ud2
puts:
    ret