   .global amd64_global_irq_handler
   .type amd64_global_irq_handler, @function
amd64_global_irq_handler:
    push rax
    push rdi
    push rsi
    push rdx
    push rcx
    push r8
    push r9
    push r10
    push r11
    # TODO: `swapgs`
    mov rdi, rsp
    add rdi, 72
    sti
    call amd64_handler2
    pop r11
    pop r10
    pop r9
    pop r8
    pop rcx
    pop rdx
    pop rsi
    pop rdi
    pop rax
    add rsp, 16
    iretq
    .size amd64_global_irq_handler, .-amd64_global_irq_handler
