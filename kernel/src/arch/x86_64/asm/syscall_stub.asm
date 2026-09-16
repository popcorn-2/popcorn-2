   .section .text
   .global x86_64_syscall_stub
   .type x86_64_syscall_stub, @function
x86_64_syscall_stub:
    .cfi_startproc simple
    .cfi_register rip, rcx
    .set cfa_off, 0

    swapgs

    mov gs:[{kernel_scratch_offset}], rsp #; save userspace stack pointer

    #; 0x10               # DW_CFA_expression rule
    #; 0x07               # Target register 7 (%rsp)
    #; 0x09               # 9 bytes expression length
    #; 0x92, 0x3b, 0x00   # DW_OP_bregx (0x92), Register 59 (0x3b, %gs.base), offset 0 (0x00) /* fixme(unwind) doesn't support register 59 */
    #; 0x0c, ...          # DW_OP_const4u (0x0c), followed by 4 bytes of little-endian offset
    #; 0x22               # DW_OP_plus (pops the two values, adds them, pushes final address)

    .cfi_escape 0x10, 0x07, 0x09, 0x92, 0x3b, 0x00, 0x0c, ({kernel_scratch_offset} & 0xff), (({kernel_scratch_offset} >> 8) & 0xff), (({kernel_scratch_offset} >> 16) & 0xff), (({kernel_scratch_offset} >> 24) & 0xff), 0x22

    mov rsp, gs:[{rsp0_offset}] #; load kernel stack from TLS block
    .cfi_def_cfa rsp, 0

    #; sti #; can take interrupts now that stack is sorted
                       #; todo: fix for NMI stuff

    call {entrypoint} #; return val already in rdx:rax

    #; cli TODO: can we use interruots?

    cmp byte ptr gs:[{needs_resched_offset}], 1
    je 2f

    mov rsp, gs:[{kernel_scratch_offset}]
    .cfi_restore rsp

    swapgs
    sysretq

2:
#; ENSURE THIS MATCHES IRQ ENTRY
    push {data_segment}
    push gs:[{kernel_scratch_offset}] #; userspace rsp
    push r11 #; actually rflags
    push {code_segment}
    push rcx #; actually rip
    push 0 #; dummy value for error code
    push 0 #; dummy value for interrupt vector
    push rax
    push rbx
    push rcx
    push rdx
    push rsi
    push rdi
    push rbp
    push r8
    push r9
    push r10
    push r11
    push r12
    push r13
    push r14
    push r15

    jmp x86_64_kernel_exit_to_userspace

    .cfi_endproc
    .size x86_64_syscall_stub, .-x86_64_syscall_stub
