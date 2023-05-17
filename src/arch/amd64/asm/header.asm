bits 32
section .multiboot2
mb_header_start:
dd 0xE85250D6 ; magic
dd 0 ; arch
dd mb_header_end - mb_header_start ; len
dd -(0xE85250D6 + 0 + (mb_header_end - mb_header_start)) ; checksum

; tags
align 8
dw 5 ; framebuffer tag
dw 0 ; flags
dd 20 ; size
dd 0 ; 640 ; width
dd 0 ; 480 ; height
dd 0 ; 24; bpp

align 8
dw 0 ; end tag
dw 0 ; flags
dd 8 ; size
mb_header_end:
