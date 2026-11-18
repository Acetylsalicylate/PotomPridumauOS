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