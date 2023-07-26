bits 32

section .bss

global initial_mem_map_start
global initial_mem_map_end

align 0x200000
initial_mem_map_start:
resb 0x400000
initial_mem_map_end:
