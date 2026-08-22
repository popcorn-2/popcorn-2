   .global x86_64_interrupt_stub
   .type x86_64_interrupt_stub, @function
x86_64_interrupt_stub:
    .cfi_startproc
    .cfi_def_cfa rsp, 56 # CFA = rsp + 7*8 (top of interrupt frame)
    .cfi_offset rip, -40 # %RA = CFA - 5*8 (offset of rip in stack frame)
    .cfi_offset rsp, -16 # %RSP = CFA - 2*8 (offset of rsp in stack frame)

    cmp byte ptr [rsp + 24], 0x8 # check if CS == kernel CS (24 because CS is at 8, and then we also have error code + vector)
    je 2f
    swapgs # if not, we come from userspace, so swap GS and kernel GS
    2:

    push rax
    .cfi_def_cfa_offset 64
    .cfi_offset rax, -64
    push rdi
    .cfi_def_cfa_offset 72
    .cfi_offset rdi, -72
    push rsi
    .cfi_def_cfa_offset 80
    .cfi_offset rsi, -80
    push rdx
    .cfi_def_cfa_offset 88
    .cfi_offset rdx, -88
    push rcx
    .cfi_def_cfa_offset 96
    .cfi_offset rcx, -96
    push r8
    .cfi_def_cfa_offset 104
    .cfi_offset r8, -104
    push r9
    .cfi_def_cfa_offset 112
    .cfi_offset r9, -112
    push r10
    .cfi_def_cfa_offset 120
    .cfi_offset r10, -120
    push r11
    .cfi_def_cfa_offset 128
    .cfi_offset r11, -128
    mov rdi, rsp
    sti
    call {entrypoint}
    pop r11
    .cfi_def_cfa_offset 120
    .cfi_same_value r11
    pop r10
    .cfi_def_cfa_offset 112
    .cfi_same_value r10
    pop r9
    .cfi_def_cfa_offset 104
    .cfi_same_value r9
    pop r8
    .cfi_def_cfa_offset 96
    .cfi_same_value r8
    pop rcx
    .cfi_def_cfa_offset 88
    .cfi_same_value rcx
    pop rdx
    .cfi_def_cfa_offset 80
    .cfi_same_value rdx
    pop rsi
    .cfi_def_cfa_offset 72
    .cfi_same_value rsi
    pop rdi
    .cfi_def_cfa_offset 64
    .cfi_same_value rdi
    pop rax
    .cfi_def_cfa_offset 56
    .cfi_same_value rax
    add rsp, 16
    .cfi_def_cfa_offset 40

    cmp byte ptr [rsp + 8], 0x8 # check if CS == kernel CS (already popped vector num and error code so only +8)
    je 3f
    swapgs # if not, we come from userspace, so swap GS and kernel GS
    3:

    iretq
    .cfi_endproc
    .size x86_64_interrupt_stub, .-x86_64_interrupt_stub
