   .section .text
   .global x86_64_syscall_stub
   .type x86_64_syscall_stub, @function
x86_64_syscall_stub:
    .cfi_startproc simple
    .cfi_register rip, rcx

    swapgs

    mov r12, rsp # save userspace stack pointer
    .cfi_register rsp, 12
    mov rsp, gs:[{rsp0_offset}] # load kernel stack from TLS block
    .cfi_def_cfa rsp, 0

    push rcx
    .cfi_def_cfa_offset 8
    .cfi_offset rcx, -8
    .cfi_offset rip, -8 # fixme: unwinder bug? - dependencies not properly evaluated
    push r11
    .cfi_def_cfa_offset 16
    .cfi_offset r11, -16

    mov r11, rsp
    sub rsp, 32

    mov [rsp+16], r15 // async_data

    mov [rsp+8], r11 // stack

    mov [rsp], rax // num_low
    .cfi_def_cfa_offset 48
    .cfi_offset rax, -48

    mov rcx, r10

    sti # can take interrupts now that stack is sorted
                   # todo: fix for NMI stuff

    call {entrypoint} # extern C function so return val already in rdx:rax

    add rsp, 32
    .cfi_def_cfa_offset 16
    pop r11
    .cfi_def_cfa_offset 8
    .cfi_same_value r11
    pop rcx
    .cfi_def_cfa_offset 0
    .cfi_same_value rcx

    cli

    mov rsp, r12
    .cfi_register 12, rsp

    swapgs
    sysretq
    .cfi_endproc
    .size x86_64_syscall_stub, .-x86_64_syscall_stub
