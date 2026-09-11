.macro push_cfi reg
    push \reg
    .set cfa_off, cfa_off + 8
    .cfi_def_cfa_offset cfa_off
    .cfi_offset \reg, -cfa_off
.endm

.macro pop_cfi reg
    pop \reg
    .cfi_restore \reg
    .set cfa_off, cfa_off - 8
    .cfi_def_cfa_offset cfa_off
.endm

   .global x86_64_interrupt_stub
   .type x86_64_interrupt_stub, @function
   .section .text
x86_64_interrupt_stub:
    .cfi_startproc
    .cfi_def_cfa rsp, 56 #; 5 u64s pushed by cpu, plus vector and error number to make 7 in total
    .set cfa_off, 56
    .cfi_offset rip, -40 #; `rip` is 5th entry in stack frame
    .cfi_offset rsp, -16 #; `rsp` is 2nd entry in stack frame

    cmp byte ptr [rsp + 24], {code_segment} #; check if CS == kernel CS (24 because CS is at 8, and then we also have error code + vector)
    je 2f
    swapgs #; if not, we come from userspace, so swap GS and kernel GS
    2:

    push_cfi rax
    push_cfi rbx
    push_cfi rcx
    push_cfi rdx
    push_cfi rsi
    push_cfi rdi
    push_cfi rbp
    push_cfi r8
    push_cfi r9
    push_cfi r10
    push_cfi r11
    push_cfi r12
    push_cfi r13
    push_cfi r14
    push_cfi r15
    mov r12, rsp
    .cfi_register rsp, r12
    sti

    call {entrypoint}

    cli

    cmp byte ptr [rsp + 144], {code_segment} #; check if CS == kernel CS (already popped vector num and error code so only +8)
    je 3f
    swapgs #; then swap gs back to userspace gs
    3:

    pop_cfi r15
    pop_cfi r14
    pop_cfi r13
    pop_cfi r12
    pop_cfi r11
    pop_cfi r10
    pop_cfi r9
    pop_cfi r8
    pop_cfi rbp
    pop_cfi rdi
    pop_cfi rsi
    pop_cfi rdx
    pop_cfi rcx
    pop_cfi rbx
    pop_cfi rax
    add rsp, 16 #; pop vector number and error code
    .cfi_def_cfa_offset 40

    iretq
    .cfi_endproc
    .size x86_64_interrupt_stub, .-x86_64_interrupt_stub
