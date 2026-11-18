AS      := nasm
LD      := ld
CARGO   := cargo

ASFLAGS := -f elf64
LDFLAGS := -m elf_x86_64 -T linker.ld -nostdlib \
           --build-id=none -z max-page-size=0x1000 -z noexecstack --gc-sections

TARGET    := x86_64-unknown-none
RUST_LIB  := target/$(TARGET)/release/libPotomPridumauOS.a
BUILD_DIR := build
ISO_DIR   := $(BUILD_DIR)/isodir
KERNEL    := $(BUILD_DIR)/kernel.elf
ISO       := $(BUILD_DIR)/PotomPridumauOS.iso

.PHONY: all rust run clean

all: $(ISO)

$(BUILD_DIR):
	mkdir -p $(BUILD_DIR)

$(BUILD_DIR)/boot.o: boot.asm | $(BUILD_DIR)
	$(AS) $(ASFLAGS) $< -o $@

rust:
	$(CARGO) build --release

$(RUST_LIB): rust

$(KERNEL): $(BUILD_DIR)/boot.o $(RUST_LIB) linker.ld
	$(LD) $(LDFLAGS) $(BUILD_DIR)/boot.o $(RUST_LIB) -o $@
	grub-file --is-x86-multiboot2 $@

$(ISO): $(KERNEL) grub.cfg
	mkdir -p $(ISO_DIR)/boot/grub
	cp $(KERNEL) $(ISO_DIR)/boot/kernel.elf
	cp grub.cfg $(ISO_DIR)/boot/grub/grub.cfg
	grub-mkrescue -o $@ $(ISO_DIR)

run: $(ISO)
	qemu-system-x86_64 -cdrom $(ISO)

clean:
	$(CARGO) clean
	rm -rf $(BUILD_DIR)