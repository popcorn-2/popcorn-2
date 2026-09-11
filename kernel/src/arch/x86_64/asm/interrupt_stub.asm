   .global x86_64_interrupt_stub
   .type x86_64_interrupt_stub, @function
   .section .text
x86_64_interrupt_stub:
    cmp byte ptr [rsp + 24], {code_segment} #; check if CS == kernel CS (24 because CS is at 8, and then we also have error code + vector)
    je 2f
    swapgs #; if not, we come from userspace, so swap GS and kernel GS
    2:

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
    mov r12, rsp
    sti

    call {entrypoint}

    cli

    cmp byte ptr [rsp + 144], {code_segment} #; check if CS == kernel CS (already popped vector num and error code so only +8)
    je 3f
    swapgs #; then swap gs back to userspace gs
    3:

    pop r15
    pop r14
    pop r13
    pop r12
    pop r11
    pop r10
    pop r9
    pop r8
    pop rbp
    pop rdi
    pop rsi
    pop rdx
    pop rcx
    pop rbx
    pop rax
    add rsp, 16 #; pop vector number and error code

    iretq
    .size x86_64_interrupt_stub, .-x86_64_interrupt_stub
