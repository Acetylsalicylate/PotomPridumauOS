bits 32

MB2_MAGIC    equ 0xE85250D6
MB2_ARCH     equ 0                  ; 0 = i386 protected mode
MB2_LEN      equ header_end - header_start
MB2_CHECKSUM equ 0x100000000 - (MB2_MAGIC + MB2_ARCH + MB2_LEN)

section .multiboot2 align=8
header_start:
    dd MB2_MAGIC
    dd MB2_ARCH
    dd MB2_LEN
    dd MB2_CHECKSUM

    ; nessesary ending tag
    dw 0                            ; type = 0
    dw 0                            ; flags = 0
    dd 8                            ; size = 8
header_end:

section .bss
align 4096
pml4_table:
    resb 4096
pdpt_table:
    resb 4096
pd_table:
    resb 4096
stack_bottom:
    resb 16384                      ; 16 КБ for stack
stack_top:

section .rodata
align 8
gdt64:
    dq 0                                                ; zero descriptor
.code: equ $ - gdt64
    dq (1 << 43) | (1 << 44) | (1 << 47) | (1 << 53)    ; 64-bit code segment
.data: equ $ - gdt64
    dq (1 << 41) | (1 << 44) | (1 << 47)                ; 64-bit data segment
.pointer:
    dw $ - gdt64 - 1
    dq gdt64

section .text
global _start
extern kernel_main

_start:
    mov esp, stack_top

    ; setting table (identity map first 1 Gb for 2 Mb)
    mov eax, pdpt_table
    or eax, 0b11                    ; Present + Writable
    mov [pml4_table], eax

    mov eax, pd_table
    or eax, 0b11
    mov [pdpt_table], eax

    xor ecx, ecx
.map_pd:
    mov eax, 0x200000               ; 2 Mb
    mul ecx
    or eax, 0b10000011              ; Present + Writable + Huge Page (2 Mb)
    mov [pd_table + ecx * 8], eax
    inc ecx
    cmp ecx, 512
    jne .map_pd

    ; loading PML4 into CR3
    mov eax, pml4_table
    mov cr3, eax

    ; enabling PAE (CR4.PAE)
    mov eax, cr4
    or eax, 1 << 5
    mov cr4, eax

    ; enabling Long Mode (EFER.LME)
    mov ecx, 0xC0000080
    rdmsr
    or eax, 1 << 8
    wrmsr

    ; enabling Paging (CR0.PG)
    mov eax, cr0
    or eax, 1 << 31
    mov cr0, eax

    ; loading 64-bit GDT and jump into 64-bit code segment
    lgdt [gdt64.pointer]
    jmp gdt64.code:long_mode_start

bits 64
long_mode_start:
    mov ax, gdt64.data
    mov ss, ax
    mov ds, ax
    mov es, ax
    mov fs, ax
    mov gs, ax

    mov rsp, stack_top              ; guarantee an aligned 64-bit stack
    cld
    call kernel_main

    cli
.hang:
    hlt
    jmp .hang

; ─── Keyboard IRQ stub (IRQ1 = INT 33) ──────────────────────────────────────
global keyboard_isr_stub
extern keyboard_handler_inner

keyboard_isr_stub:
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

    cld
    call keyboard_handler_inner

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

    iretq

; ─── Default IRQ stub (sends EOI to PIC1 and returns) ───────────────────────
global default_irq_stub
default_irq_stub:
    push rax
    mov al, 0x20
    out 0x20, al
    pop rax
    iretq

; ─── Default ISR stub (just iretq) ──────────────────────────────────────────
global default_isr_stub
default_isr_stub:
    iretq

; ─── CPU Exception ISR stubs ────────────────────────────────────────────────
; Each stub pushes the vector number into RDI (first arg for System V ABI)
; and calls the Rust exception_handler_inner(vector: u64)

extern exception_handler_inner

%macro isr_no_err_stub 1
global isr_stub_%1
isr_stub_%1:
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

    mov rdi, %1              ; vector number as first argument
    call exception_handler_inner

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
    iretq
%endmacro

; Exceptions without error code
isr_no_err_stub 0
isr_no_err_stub 1
isr_no_err_stub 2
isr_no_err_stub 3
isr_no_err_stub 4
isr_no_err_stub 5
isr_no_err_stub 6
isr_no_err_stub 7

; Exception 8 (Double Fault) has an error code, but we just discard it
global isr_stub_8
isr_stub_8:
    ; CPU pushes error code on stack before the interrupt frame
    ; We pop it off so our generic handler works
    add rsp, 8               ; discard error code
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
    mov rdi, 8
    call exception_handler_inner
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
    iretq

; Exceptions 10-14 push error codes
%macro isr_err_stub 1
global isr_stub_%1
isr_stub_%1:
    add rsp, 8               ; discard error code
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
    mov rdi, %1
    call exception_handler_inner
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
    iretq
%endmacro

isr_err_stub 10
isr_err_stub 11
isr_err_stub 12
isr_err_stub 13
isr_err_stub 14