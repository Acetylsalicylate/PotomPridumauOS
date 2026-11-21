#![no_std]
#![allow(non_snake_case)]

use core::arch::asm;
use core::fmt::{self, Write};
use core::panic::PanicInfo;

// ─── Port I/O ───────────────────────────────────────────────────────────────

#[inline]
unsafe fn outb(port: u16, val: u8) {
    unsafe {
        asm!("out dx, al", in("dx") port, in("al") val, options(nomem, nostack, preserves_flags));
    }
}

#[inline]
unsafe fn inb(port: u16) -> u8 {
    let val: u8;
    unsafe {
        asm!("in al, dx", out("al") val, in("dx") port, options(nomem, nostack, preserves_flags));
    }
    val
}

#[inline]
unsafe fn io_wait() {
    unsafe {
        outb(0x80, 0);
    }
}

// ─── VGA Text Mode ─────────────────────────────────────────────────────────

const VGA_WIDTH: usize = 80;
const VGA_HEIGHT: usize = 25;
const VGA_BUFFER: *mut u16 = 0xB8000 as *mut u16;

#[allow(dead_code)]
#[repr(u8)]
#[derive(Clone, Copy)]
pub enum Color {
    Black = 0,
    Blue = 1,
    Green = 2,
    Cyan = 3,
    Red = 4,
    Magenta = 5,
    Brown = 6,
    LightGrey = 7,
    DarkGrey = 8,
    LightBlue = 9,
    LightGreen = 10,
    LightCyan = 11,
    LightRed = 12,
    Pink = 13,
    Yellow = 14,
    White = 15,
}

#[derive(Clone, Copy)]
struct ColorCode(u8);

impl ColorCode {
    const fn new(fg: Color, bg: Color) -> Self {
        Self(((bg as u8) << 4) | (fg as u8))
    }
}

pub struct VgaWriter {
    row: usize,
    col: usize,
    color: ColorCode,
}

impl VgaWriter {
    pub fn new() -> Self {
        let mut writer = Self {
            row: 0,
            col: 0,
            color: ColorCode::new(Color::LightGrey, Color::Black),
        };
        writer.clear();
        writer
    }

    pub fn set_color(&mut self, fg: Color, bg: Color) {
        self.color = ColorCode::new(fg, bg);
    }

    pub fn clear(&mut self) {
        let blank = self.vga_entry(b' ');
        for i in 0..(VGA_WIDTH * VGA_HEIGHT) {
            unsafe {
                VGA_BUFFER.add(i).write_volatile(blank);
            }
        }
        self.row = 0;
        self.col = 0;
    }

    fn vga_entry(&self, ch: u8) -> u16 {
        (ch as u16) | ((self.color.0 as u16) << 8)
    }

    fn new_line(&mut self) {
        self.col = 0;
        if self.row + 1 < VGA_HEIGHT {
            self.row += 1;
        } else {
            self.scroll();
        }
    }

    fn scroll(&mut self) {
        for y in 0..(VGA_HEIGHT - 1) {
            for x in 0..VGA_WIDTH {
                unsafe {
                    let val = VGA_BUFFER.add((y + 1) * VGA_WIDTH + x).read_volatile();
                    VGA_BUFFER.add(y * VGA_WIDTH + x).write_volatile(val);
                }
            }
        }
        let blank = self.vga_entry(b' ');
        for x in 0..VGA_WIDTH {
            unsafe {
                VGA_BUFFER
                    .add((VGA_HEIGHT - 1) * VGA_WIDTH + x)
                    .write_volatile(blank);
            }
        }
        self.row = VGA_HEIGHT - 1;
    }

    pub fn write_byte(&mut self, byte: u8) {
        match byte {
            b'\n' => self.new_line(),
            b'\r' => self.col = 0,
            0x08 => {
                // Backspace
                if self.col > 0 {
                    self.col -= 1;
                    let blank = self.vga_entry(b' ');
                    unsafe {
                        VGA_BUFFER
                            .add(self.row * VGA_WIDTH + self.col)
                            .write_volatile(blank);
                    }
                }
            }
            byte => {
                if self.col >= VGA_WIDTH {
                    self.new_line();
                }
                let entry = self.vga_entry(byte);
                unsafe {
                    VGA_BUFFER
                        .add(self.row * VGA_WIDTH + self.col)
                        .write_volatile(entry);
                }
                self.col += 1;
            }
        }
    }

    /// Update the hardware cursor position
    pub fn update_cursor(&self) {
        let pos = (self.row * VGA_WIDTH + self.col) as u16;
        unsafe {
            outb(0x3D4, 14);
            outb(0x3D5, (pos >> 8) as u8);
            outb(0x3D4, 15);
            outb(0x3D5, (pos & 0xFF) as u8);
        }
    }

    /// Enable the VGA hardware cursor (underline style)
    pub fn enable_cursor(&self) {
        unsafe {
            outb(0x3D4, 0x0A);
            let cur = inb(0x3D5);
            outb(0x3D5, (cur & 0xC0) | 13); // cursor start scanline
            outb(0x3D4, 0x0B);
            let cur = inb(0x3D5);
            outb(0x3D5, (cur & 0xE0) | 15); // cursor end scanline
        }
    }
}

impl Write for VgaWriter {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for byte in s.bytes() {
            match byte {
                0x20..=0x7E | b'\n' | b'\r' | 0x08 => self.write_byte(byte),
                _ => self.write_byte(b'?'),
            }
        }
        Ok(())
    }
}

// ─── PIC 8259 ───────────────────────────────────────────────────────────────

const PIC1_CMD: u16 = 0x20;
const PIC1_DATA: u16 = 0x21;
const PIC2_CMD: u16 = 0xA0;
const PIC2_DATA: u16 = 0xA1;

/// Remap the PIC so that IRQ0–7 → INT 32–39, IRQ8–15 → INT 40–47
unsafe fn pic_remap() {
    unsafe {
        // ICW1: begin initialization (cascade mode, ICW4 needed)
        outb(PIC1_CMD, 0x11);
        io_wait();
        outb(PIC2_CMD, 0x11);
        io_wait();

        // ICW2: vector offsets
        outb(PIC1_DATA, 0x20); // IRQ 0-7  → INT 32-39
        io_wait();
        outb(PIC2_DATA, 0x28); // IRQ 8-15 → INT 40-47
        io_wait();

        // ICW3: tell Master about Slave on IRQ2, tell Slave its cascade identity
        outb(PIC1_DATA, 0x04);
        io_wait();
        outb(PIC2_DATA, 0x02);
        io_wait();

        // ICW4: 8086 mode
        outb(PIC1_DATA, 0x01);
        io_wait();
        outb(PIC2_DATA, 0x01);
        io_wait();

        // Mask all IRQs except IRQ1 (keyboard: bit 1 = 0)
        outb(PIC1_DATA, 0xFD); // 1111_1101b
        outb(PIC2_DATA, 0xFF); // 1111_1111b (disable all on PIC2)
    }
}

/// Send End-Of-Interrupt to the PIC(s)
unsafe fn pic_eoi(irq: u8) {
    unsafe {
        if irq >= 8 {
            outb(PIC2_CMD, 0x20);
        }
        outb(PIC1_CMD, 0x20);
    }
}

// ─── IDT (Interrupt Descriptor Table) ───────────────────────────────────────

#[repr(C)]
#[derive(Clone, Copy)]
struct IdtEntry {
    offset_low: u16,
    selector: u16,
    ist: u8,
    type_attr: u8,
    offset_mid: u16,
    offset_high: u32,
    reserved: u32,
}

impl IdtEntry {
    const fn empty() -> Self {
        Self {
            offset_low: 0,
            selector: 0,
            ist: 0,
            type_attr: 0,
            offset_mid: 0,
            offset_high: 0,
            reserved: 0,
        }
    }

    fn set_handler(&mut self, handler: usize) {
        self.offset_low = handler as u16;
        self.offset_mid = (handler >> 16) as u16;
        self.offset_high = (handler >> 32) as u32;
        self.selector = 0x08; // code segment selector from GDT
        self.ist = 0;
        self.type_attr = 0x8E; // present, DPL=0, 64-bit interrupt gate
        self.reserved = 0;
    }
}

#[repr(C, packed)]
struct IdtPointer {
    limit: u16,
    base: u64,
}

const IDT_SIZE: usize = 256;

#[repr(C, align(16))]
struct IdtTable([IdtEntry; IDT_SIZE]);

static mut IDT: IdtTable = IdtTable([IdtEntry::empty(); IDT_SIZE]);

unsafe fn idt_load() {
    let ptr = IdtPointer {
        limit: (core::mem::size_of::<IdtTable>() - 1) as u16,
        base: &raw const IDT as u64,
    };
    unsafe {
        asm!("lidt [{}]", in(reg) &ptr, options(readonly, nostack, preserves_flags));
    }
}

// ─── Default CPU exception handler (called from asm stubs) ─────────────────

#[unsafe(no_mangle)]
pub extern "C" fn exception_handler_inner(vector: u64) {
    // Paint the top line red with the exception number
    let mut vga = VgaWriter {
        row: 0,
        col: 0,
        color: ColorCode::new(Color::White, Color::Red),
    };
    for i in 0..VGA_WIDTH {
        unsafe {
            VGA_BUFFER.add(i).write_volatile(((Color::Red as u16) << 12) | (b' ' as u16));
        }
    }
    let _ = write!(vga, " EXCEPTION #{} — SYSTEM HALTED ", vector);

    loop {
        unsafe {
            asm!("cli; hlt", options(nomem, nostack));
        }
    }
}

// ─── Keyboard state ─────────────────────────────────────────────────────────

static mut LEFT_SHIFT: bool = false;
static mut RIGHT_SHIFT: bool = false;
static mut CAPS_LOCK: bool = false;

/// PS/2 Scan Code Set 1 → ASCII (lowercase / unshifted)
const SCANCODE_TO_ASCII: [u8; 128] = {
    let mut table = [0u8; 128];
    // Row 1: number row
    table[0x02] = b'1';
    table[0x03] = b'2';
    table[0x04] = b'3';
    table[0x05] = b'4';
    table[0x06] = b'5';
    table[0x07] = b'6';
    table[0x08] = b'7';
    table[0x09] = b'8';
    table[0x0A] = b'9';
    table[0x0B] = b'0';
    table[0x0C] = b'-';
    table[0x0D] = b'=';
    table[0x0E] = 0x08; // Backspace
    table[0x0F] = b'\t';
    // Row 2: QWERTY
    table[0x10] = b'q';
    table[0x11] = b'w';
    table[0x12] = b'e';
    table[0x13] = b'r';
    table[0x14] = b't';
    table[0x15] = b'y';
    table[0x16] = b'u';
    table[0x17] = b'i';
    table[0x18] = b'o';
    table[0x19] = b'p';
    table[0x1A] = b'[';
    table[0x1B] = b']';
    table[0x1C] = b'\n'; // Enter
    // Row 3: ASDF
    table[0x1E] = b'a';
    table[0x1F] = b's';
    table[0x20] = b'd';
    table[0x21] = b'f';
    table[0x22] = b'g';
    table[0x23] = b'h';
    table[0x24] = b'j';
    table[0x25] = b'k';
    table[0x26] = b'l';
    table[0x27] = b';';
    table[0x28] = b'\'';
    table[0x29] = b'`';
    table[0x2B] = b'\\';
    // Row 4: ZXCV
    table[0x2C] = b'z';
    table[0x2D] = b'x';
    table[0x2E] = b'c';
    table[0x2F] = b'v';
    table[0x30] = b'b';
    table[0x31] = b'n';
    table[0x32] = b'm';
    table[0x33] = b',';
    table[0x34] = b'.';
    table[0x35] = b'/';
    // Space
    table[0x39] = b' ';
    table
};

/// Shifted variants of the number/symbol keys
const SCANCODE_TO_ASCII_SHIFTED: [u8; 128] = {
    let mut table = [0u8; 128];
    // Number row shifted → symbols
    table[0x02] = b'!';
    table[0x03] = b'@';
    table[0x04] = b'#';
    table[0x05] = b'$';
    table[0x06] = b'%';
    table[0x07] = b'^';
    table[0x08] = b'&';
    table[0x09] = b'*';
    table[0x0A] = b'(';
    table[0x0B] = b')';
    table[0x0C] = b'_';
    table[0x0D] = b'+';
    table[0x0E] = 0x08; // Backspace stays the same
    table[0x0F] = b'\t';
    // Letters → uppercase
    table[0x10] = b'Q';
    table[0x11] = b'W';
    table[0x12] = b'E';
    table[0x13] = b'R';
    table[0x14] = b'T';
    table[0x15] = b'Y';
    table[0x16] = b'U';
    table[0x17] = b'I';
    table[0x18] = b'O';
    table[0x19] = b'P';
    table[0x1A] = b'{';
    table[0x1B] = b'}';
    table[0x1C] = b'\n';
    table[0x1E] = b'A';
    table[0x1F] = b'S';
    table[0x20] = b'D';
    table[0x21] = b'F';
    table[0x22] = b'G';
    table[0x23] = b'H';
    table[0x24] = b'J';
    table[0x25] = b'K';
    table[0x26] = b'L';
    table[0x27] = b':';
    table[0x28] = b'"';
    table[0x29] = b'~';
    table[0x2B] = b'|';
    table[0x2C] = b'Z';
    table[0x2D] = b'X';
    table[0x2E] = b'C';
    table[0x2F] = b'V';
    table[0x30] = b'B';
    table[0x31] = b'N';
    table[0x32] = b'M';
    table[0x33] = b'<';
    table[0x34] = b'>';
    table[0x35] = b'?';
    table[0x39] = b' ';
    table
};

/// Returns true if scancode corresponds to an alphabetic key (a-z)
fn is_alpha_scancode(scancode: u8) -> bool {
    matches!(
        scancode,
        0x10..=0x19 | 0x1E..=0x26 | 0x2C..=0x32
    )
}

/// Translate a PS/2 scancode to an ASCII character, applying Shift and Caps Lock
fn scancode_to_char(scancode: u8) -> Option<u8> {
    if scancode >= 128 {
        return None;
    }

    let shift_held = unsafe { LEFT_SHIFT || RIGHT_SHIFT };
    let caps = unsafe { CAPS_LOCK };

    let ch = if is_alpha_scancode(scancode) {
        // For letters: Shift XOR CapsLock → uppercase
        if shift_held ^ caps {
            SCANCODE_TO_ASCII_SHIFTED[scancode as usize]
        } else {
            SCANCODE_TO_ASCII[scancode as usize]
        }
    } else {
        // For non-letters: only Shift matters
        if shift_held {
            SCANCODE_TO_ASCII_SHIFTED[scancode as usize]
        } else {
            SCANCODE_TO_ASCII[scancode as usize]
        }
    };

    if ch == 0 {
        None
    } else {
        Some(ch)
    }
}

// ─── Global VGA writer for the keyboard handler ─────────────────────────────

struct UnsafeVga {
    inner: core::cell::UnsafeCell<Option<VgaWriter>>,
}

unsafe impl Sync for UnsafeVga {}

impl UnsafeVga {
    const fn new() -> Self {
        Self {
            inner: core::cell::UnsafeCell::new(None),
        }
    }

    unsafe fn set(&self, vga: VgaWriter) {
        unsafe {
            *self.inner.get() = Some(vga);
        }
    }

    unsafe fn get(&self) -> &mut VgaWriter {
        unsafe { (*self.inner.get()).as_mut().unwrap() }
    }
}

static GLOBAL_VGA: UnsafeVga = UnsafeVga::new();

unsafe fn get_vga() -> &'static mut VgaWriter {
    unsafe { GLOBAL_VGA.get() }
}

// ─── Keyboard IRQ handler (IRQ1 = INT 33) ───────────────────────────────────

#[unsafe(no_mangle)]
pub extern "C" fn keyboard_handler_inner() {
    let scancode = unsafe { inb(0x60) };

    // Key release (bit 7 set)
    if scancode & 0x80 != 0 {
        let released = scancode & 0x7F;
        match released {
            0x2A => unsafe { LEFT_SHIFT = false },  // Left Shift released
            0x36 => unsafe { RIGHT_SHIFT = false }, // Right Shift released
            _ => {}
        }
    } else {
        // Key press
        match scancode {
            0x2A => unsafe { LEFT_SHIFT = true },  // Left Shift pressed
            0x36 => unsafe { RIGHT_SHIFT = true }, // Right Shift pressed
            0x3A => {
                // Caps Lock pressed — toggle
                unsafe {
                    CAPS_LOCK = !CAPS_LOCK;
                }
            }
            _ => {
                if let Some(ch) = scancode_to_char(scancode) {
                    let vga = unsafe { get_vga() };
                    vga.write_byte(ch);
                    vga.update_cursor();
                }
            }
        }
    }

    unsafe {
        pic_eoi(1);
    }
}

// ─── IDT initialization ─────────────────────────────────────────────────────

unsafe extern "C" {
    fn isr_stub_0();
    fn isr_stub_1();
    fn isr_stub_2();
    fn isr_stub_3();
    fn isr_stub_4();
    fn isr_stub_5();
    fn isr_stub_6();
    fn isr_stub_7();
    fn isr_stub_8();
    fn isr_stub_10();
    fn isr_stub_11();
    fn isr_stub_12();
    fn isr_stub_13();
    fn isr_stub_14();
    fn keyboard_isr_stub();
    fn default_irq_stub();
    fn default_isr_stub();
}

unsafe fn idt_init() {
    unsafe {
        // First fill all 256 entries with a default stub to prevent #GP on unexpected interrupts
        for i in 0..IDT_SIZE {
            IDT.0[i].set_handler(default_isr_stub as *const () as usize);
        }

        // PIC IRQ entries (32..48) default to sending EOI to PIC1
        for i in 32..48 {
            IDT.0[i].set_handler(default_irq_stub as *const () as usize);
        }

        // CPU exceptions
        IDT.0[0].set_handler(isr_stub_0 as *const () as usize);
        IDT.0[1].set_handler(isr_stub_1 as *const () as usize);
        IDT.0[2].set_handler(isr_stub_2 as *const () as usize);
        IDT.0[3].set_handler(isr_stub_3 as *const () as usize);
        IDT.0[4].set_handler(isr_stub_4 as *const () as usize);
        IDT.0[5].set_handler(isr_stub_5 as *const () as usize);
        IDT.0[6].set_handler(isr_stub_6 as *const () as usize);
        IDT.0[7].set_handler(isr_stub_7 as *const () as usize);
        IDT.0[8].set_handler(isr_stub_8 as *const () as usize);
        IDT.0[10].set_handler(isr_stub_10 as *const () as usize);
        IDT.0[11].set_handler(isr_stub_11 as *const () as usize);
        IDT.0[12].set_handler(isr_stub_12 as *const () as usize);
        IDT.0[13].set_handler(isr_stub_13 as *const () as usize);
        IDT.0[14].set_handler(isr_stub_14 as *const () as usize);

        // IRQ1 → INT 33 (keyboard)
        IDT.0[33].set_handler(keyboard_isr_stub as *const () as usize);

        idt_load();
    }
}

// ─── Kernel Entry ───────────────────────────────────────────────────────────

#[unsafe(no_mangle)]
pub extern "C" fn kernel_main() -> ! {
    let mut vga = VgaWriter::new();
    vga.enable_cursor();

    vga.set_color(Color::LightGreen, Color::Black);
    let _ = writeln!(vga, "PotomPridumauOS v0.1");

    vga.set_color(Color::LightCyan, Color::Black);
    let _ = writeln!(vga, "Booted via GRUB2 Multiboot2 into 64-bit Long Mode.");

    vga.set_color(Color::Yellow, Color::Black);
    let _ = writeln!(vga, "Initializing IDT...");

    unsafe {
        pic_remap();
        idt_init();
    }

    vga.set_color(Color::LightGreen, Color::Black);
    let _ = writeln!(vga, "IDT loaded. PIC remapped. Keyboard enabled.");
    let _ = writeln!(vga, "Caps Lock and Shift modifiers are supported.");
    let _ = writeln!(vga);

    vga.set_color(Color::White, Color::Black);
    let _ = write!(vga, "> ");
    vga.update_cursor();

    // Store the VGA writer globally for use in interrupt handlers
    unsafe {
        GLOBAL_VGA.set(vga);
    }

    // Drain any leftover scancodes from PS/2 buffer before unmasking CPU interrupts
    unsafe {
        while (inb(0x64) & 0x01) != 0 {
            let _ = inb(0x60);
        }
    }

    // Enable interrupts and halt in a loop
    unsafe {
        asm!("sti", options(nomem, nostack));
    }

    loop {
        unsafe {
            asm!("hlt", options(nomem, nostack, preserves_flags));
        }
    }
}

// ─── Panic Handler ──────────────────────────────────────────────────────────

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    // Try to print panic info to screen
    let mut vga = VgaWriter {
        row: 24,
        col: 0,
        color: ColorCode::new(Color::White, Color::Red),
    };
    let _ = write!(vga, "KERNEL PANIC: {}", info);

    loop {
        unsafe {
            asm!("cli; hlt", options(nomem, nostack));
        }
    }
}

// ─── Compiler builtins ──────────────────────────────────────────────────────

#[allow(suspicious_runtime_symbol_definitions)]
mod mem {
    use core::ffi::{c_int, c_void};

    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn memcpy(dest: *mut c_void, src: *const c_void, n: usize) -> *mut c_void {
        let d = dest as *mut u8;
        let s = src as *const u8;
        let mut i = 0;
        while i < n {
            unsafe {
                d.add(i).write_volatile(s.add(i).read_volatile());
            }
            i += 1;
        }
        dest
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn memmove(dest: *mut c_void, src: *const c_void, n: usize) -> *mut c_void {
        let d = dest as *mut u8;
        let s = src as *const u8;
        if (d as usize) <= (s as usize) {
            let mut i = 0;
            while i < n {
                unsafe {
                    d.add(i).write_volatile(s.add(i).read_volatile());
                }
                i += 1;
            }
        } else {
            let mut i = n;
            while i > 0 {
                i -= 1;
                unsafe {
                    d.add(i).write_volatile(s.add(i).read_volatile());
                }
            }
        }
        dest
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn memset(s: *mut c_void, c: c_int, n: usize) -> *mut c_void {
        let dst = s as *mut u8;
        let byte = c as u8;
        let mut i = 0;
        while i < n {
            unsafe {
                dst.add(i).write_volatile(byte);
            }
            i += 1;
        }
        s
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn memcmp(s1: *const c_void, s2: *const c_void, n: usize) -> c_int {
        let p1 = s1 as *const u8;
        let p2 = s2 as *const u8;
        let mut i = 0;
        while i < n {
            let (a, b) = unsafe { (p1.add(i).read_volatile(), p2.add(i).read_volatile()) };
            if a != b {
                return (a as c_int) - (b as c_int);
            }
            i += 1;
        }
        0
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn bcmp(s1: *const c_void, s2: *const c_void, n: usize) -> c_int {
        unsafe { memcmp(s1, s2, n) }
    }
}
