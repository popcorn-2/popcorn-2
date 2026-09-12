.macro push_cfi reg
    push \reg
    .set cfa_off, cfa_off + 8
    .cfi_def_cfa_offset cfa_off
    .cfi_offset \reg, -cfa_off
.endm

.macro push_dummy val
    push \val
    .set cfa_off, cfa_off + 8
    .cfi_def_cfa_offset cfa_off
.endm

.macro pop_cfi reg
    pop \reg
    .cfi_restore \reg
    .set cfa_off, cfa_off - 8
    .cfi_def_cfa_offset cfa_off
.endm

   .section .text
   .global x86_64_syscall_stub
   .type x86_64_syscall_stub, @function
x86_64_syscall_stub:
    .cfi_startproc simple
    .cfi_register rip, rcx
    .set cfa_off, 0

    swapgs

    mov gs:[{kernel_scratch_offset}], rsp # save userspace stack pointer

    # 0x10               # DW_CFA_expression rule
    # 0x07               # Target register 7 (%rsp)
    # 0x09               # 9 bytes expression length
    # 0x92, 0x3b, 0x00   # DW_OP_bregx (0x92), Register 59 (0x3b, %gs.base), offset 0 (0x00) /* fixme(unwind) doesn't support register 59 */
    # 0x0c, ...          # DW_OP_const4u (0x0c), followed by 4 bytes of little-endian offset
    # 0x22               # DW_OP_plus (pops the two values, adds them, pushes final address)

    .cfi_escape 0x10, 0x07, 0x09, 0x92, 0x3b, 0x00, 0x0c, ({kernel_scratch_offset} & 0xff), (({kernel_scratch_offset} >> 8) & 0xff), (({kernel_scratch_offset} >> 16) & 0xff), (({kernel_scratch_offset} >> 24) & 0xff), 0x22

    mov rsp, gs:[{rsp0_offset}] # load kernel stack from TLS block
    .cfi_def_cfa rsp, 0

    sti # can take interrupts now that stack is sorted
                       # todo: fix for NMI stuff

    push_dummy 0 # needed for stack alignment
    # rax not preserved
    push_cfi rbx
    push_cfi rcx
    .cfi_offset rip, -cfa_off # fixme: unwinder bug? - dependencies not properly evaluated
    # rdx not preserved
    # rsi not preserved
    # rdi not preserved
    # rbp saved by callee
    # rsp saved by callee
    # r8 not preserved
    # r9 not preserved
    push_cfi r10
    push_cfi r11
    # r12 not preserved
    push_cfi r13
    push_cfi r14
    push_cfi r15

    call {entrypoint} # return val already in rdx:rax

    pop_cfi r15
    pop_cfi r14
    pop_cfi r13
    pop_cfi r11
    pop_cfi r10
    pop_cfi rcx
    .cfi_register rip, rcx
    pop_cfi rbx
    add rsp, 8
    .cfi_def_cfa_offset 0

    cli

    mov rsp, gs:[{kernel_scratch_offset}]
    .cfi_restore rsp

    swapgs
    sysretq

    .cfi_endproc
    .size x86_64_syscall_stub, .-x86_64_syscall_stub
