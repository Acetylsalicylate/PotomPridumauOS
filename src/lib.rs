#![no_std]
#![allow(non_snake_case)]

use core::arch::asm;
use core::fmt::{self, Write};
use core::panic::PanicInfo;

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
}

impl Write for VgaWriter {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for byte in s.bytes() {
            match byte {
                0x20..=0x7E | b'\n' | b'\r' => self.write_byte(byte),
                _ => self.write_byte(b'?'),
            }
        }
        Ok(())
    }
}


#[unsafe(no_mangle)]
pub extern "C" fn kernel_main() -> ! {
    let mut vga = VgaWriter::new();

    vga.set_color(Color::LightGreen, Color::Black);
    let _ = writeln!(vga, "Hello world!");

    vga.set_color(Color::LightGrey, Color::Black);
    let _ = writeln!(vga, "Booted via GRUB2 Multiboot2 into 64-bit Long Mode.");
    let _ = writeln!(vga, "Formatted output works: 2 + 2 = {}", 2 + 2);

    loop {
        unsafe {
            asm!("hlt", options(nomem, nostack, preserves_flags));
        }
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {
        unsafe {
            asm!("cli; hlt", options(nomem, nostack));
        }
    }
}


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