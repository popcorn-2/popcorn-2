%include "constants.inc"

bits 32

section .bss
global psf_version
global psf_headersize
global psf_flags
global psf_numglyph
global psf_bytesperglyph
global psf_height
global psf_width

psf_version: resb 4
psf_headersize: resb 4
psf_flags: resb 4
psf_numglyph: resb 4
psf_bytesperglyph: resb 4
psf_height: resb 4
psf_width: resb 4

section .text
global parse_psf
extern font_psf_start
extern font_psf_end
extern font_psf_size

PSF_FONT_MAGIC: equ 0x864ab572

parse_psf:
	; prologue
	push ebp
	mov ebp, esp
	
	; save regs
	push ebx
	push esi
	push edi
	
	
	mov dword eax, font_psf_start - KERNEL_OFFSET
	mov dword ebx, [eax]
	cmp ebx, PSF_FONT_MAGIC
	je .magic_correct
	ud2 ; if magic was wrong, abort boot
.magic_correct:
	add eax, 4
	mov dword ebx, [eax]
	mov dword [psf_version - KERNEL_OFFSET], ebx
	
	add eax, 4
	mov dword ebx, [eax]
	mov dword [psf_headersize - KERNEL_OFFSET], ebx
	
	add eax, 4
	mov dword ebx, [eax]
	mov dword [psf_flags - KERNEL_OFFSET], ebx
	
	add eax, 4
	mov dword ebx, [eax]
	mov dword [psf_numglyph - KERNEL_OFFSET], ebx
	
	add eax, 4
	mov dword ebx, [eax]
	mov dword [psf_bytesperglyph - KERNEL_OFFSET], ebx
	
	add eax, 4
	mov dword ebx, [eax]
	mov dword [psf_height - KERNEL_OFFSET], ebx
	
	add eax, 4
	mov dword ebx, [eax]
	mov dword [psf_width - KERNEL_OFFSET], ebx
	
	
	mov dword eax, [psf_version - KERNEL_OFFSET] ; sanity check
	cmp eax, 0
	je .valid_version
	ud2 ; if version != 0 then this is not a valid PSF
.valid_version:
	
	; unsave regs
	pop edi
	pop esi
	pop ebx
	
	; epilogue
	mov esp, ebp
	pop ebp
	ret
