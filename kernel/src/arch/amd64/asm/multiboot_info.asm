%include "constants.inc"

bits 32

section .bss
global framebuffer_addr
global framebuffer_pitch
global framebuffer_width
global framebuffer_height
global framebuffer_bpp
global framebuffer_type

framebuffer_addr: resb 8
framebuffer_pitch: resb 4
framebuffer_width: resb 4
framebuffer_height: resb 4
framebuffer_bpp: resb 1
framebuffer_type: resb 1


section .text
global read_multiboot_info

TAG_TYPE_END: equ 0
TAG_TYPE_FRAMEBUFFER: equ 8

read_multiboot_info:
	; prologue
	push ebp
	mov ebp, esp
	
	; save regs
	push ebx
	push esi
	push edi
	
	mov ebx, [ebp + 12] ; retrieve multiboot info struct pointer
	add ebx, 8 ; move to the beginning of multiboot data (after header)

.check_tag:
		cmp dword [ebx], TAG_TYPE_END
		je .end
		cmp dword [ebx], TAG_TYPE_FRAMEBUFFER
		je .parse_framebuffer

.continue_check_tag:
		add ebx, [ebx + 4] ; add size of tag (to get to next tag)
		add ebx, 7
		and ebx, ~7 ; ebx is now aligned up to an 8 byte boundary
		
	jmp .check_tag

.parse_framebuffer:
	mov ecx, ebx
	add ecx, 8
	mov eax, dword [ecx]
	mov dword [framebuffer_addr - KERNEL_OFFSET], eax ; lower addr
	add ecx, 4
	mov eax, dword [ecx]
	mov dword [framebuffer_addr - KERNEL_OFFSET + 4], eax ; upper addr
	add ecx, 4
	mov eax, dword [ecx]
	mov dword [framebuffer_pitch - KERNEL_OFFSET], eax
	add ecx, 4
	mov eax, dword [ecx]
	mov dword [framebuffer_width - KERNEL_OFFSET], eax
	add ecx, 4
	mov eax, dword [ecx]
	mov dword [framebuffer_height - KERNEL_OFFSET], eax
	add ecx, 4
	mov al, byte [ecx]
	mov byte [framebuffer_bpp - KERNEL_OFFSET], al
	add ecx, 1
	mov al, byte [ecx]
	mov byte [framebuffer_type - KERNEL_OFFSET], al
	jmp .continue_check_tag

.end:
	; unsave regs
	pop edi
	pop esi
	pop ebx
	
	; epilogue
	mov esp, ebp
	pop ebp
	ret