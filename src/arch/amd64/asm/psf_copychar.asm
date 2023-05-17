%include "constants.inc"

bits 32

section .bss

section .text
global psf_copychar

extern font_psf_start
extern font_psf_end
extern font_psf_size

extern psf_headersize
extern psf_numglyph
extern psf_bytesperglyph
extern psf_height
extern psf_width

extern framebuffer_addr
extern framebuffer_pitch
extern framebuffer_width
extern framebuffer_height
extern framebuffer_bpp

extern col

psf_copychar: ; (char_index, y, x)
	; prologue
	push ebp
	mov ebp, esp
	
	; save regs
	push ebx
	push esi
	push edi
	
	
	movzx eax, word [ebp + 8] ; first arg (pushed last) is char index
	cmp eax, [psf_numglyph - KERNEL_OFFSET]
	jle .valid_index
	jmp .end
.valid_index:
	
	imul dword [psf_bytesperglyph - KERNEL_OFFSET] ; clobbers edx
	add eax, font_psf_start - KERNEL_OFFSET
	add eax, [psf_headersize - KERNEL_OFFSET] ; eax now points to the relevant char in the PSF
	push eax
	
	mov eax, [framebuffer_pitch - KERNEL_OFFSET]
	imul dword [ebp + 12] ; clobbers edx, [ebp + 12] is the second argumnt (pushed second to last) which is the y coord
	imul dword [psf_height - KERNEL_OFFSET]
	; eax is now the vertical offset into the framebuffer
	mov ecx, eax
	
	mov eax, [ebp + 16] ;  third argument which is the x coord
	movzx edi, byte [framebuffer_bpp - KERNEL_OFFSET]
	shr edi, 3
	imul edi ; eax is now the horizontal offset into the framebuffer
	mov edx, [psf_width - KERNEL_OFFSET]
	inc edx
	imul edx
	
	mov esi, [framebuffer_addr - KERNEL_OFFSET]
	add esi, ecx ; add vertical offset
	add esi, eax ; add horizontal offset
	; esi is now the beginning of the location where we shall put in the character in the framebuffer
	push esi
	
	mov ecx, 0 ; y counter
.loop_y_begin:
	cmp ecx, [psf_height - KERNEL_OFFSET]
	jge .loop_y_end
	
	mov ebx, 0 ; x counter
.loop_x_begin:
	cmp ebx, [psf_width - KERNEL_OFFSET]
	jge .loop_x_end
	
	mov esi, [esp] ; esi = fb_dest
	mov eax, ecx ; eax = y coord offset
	imul dword [framebuffer_pitch - KERNEL_OFFSET]
	add esi, eax ; esi += y * pitch
	mov eax, ebx ; eax = x coord offset
	imul dword edi
	add esi, eax ; esi += x * (bpp/8)
	
	mov eax, ecx ; y
	imul dword [psf_width - KERNEL_OFFSET]
	add eax, ebx ; eax = y * width + x
	mov edx, eax
	and edx, 7 ; edx is the bit index mod 8 (aka the index of the bit in the byte)
	
	shr eax, 3 ; eax is the bit index / 8 (aka the index of the byte)
	add eax, [esp + 4] ; pointer into PSF (at correct char, now offset to be the correct byte)
	movzx ax, byte [eax]
	xchg dl, cl
	shl ax, cl
	xchg cl, dl
	mov edx, 0
	test ax, 1<<7
	cmovnz edx, [col - KERNEL_OFFSET]
	mov [esi], edx ; does write to some of the next pixel over but that's ok since we write left-to-right and leave 1px gaps between chars anyway
	
	inc ebx
	jmp .loop_x_begin
	
.loop_x_end:
	
	inc ecx
	jmp .loop_y_begin

.loop_y_end:
	add esp, 8

.end:
	; unsave regs
	pop edi
	pop esi
	pop ebx
	
	; epilogue
	mov esp, ebp
	pop ebp
	ret
