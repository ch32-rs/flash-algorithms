//! CodeFlash support for CH584X.
//!
//! CH584X has 480 KiB of CodeFlash at CPU addresses `0x0..0x77FFF`, but the
//! flash engine addresses it at `0x80000 + addr`. The official ISP585
//! `FLASH_ROM_*` path applies the same `+0x80000` offset and limits the range
//! to `0x70000`/`0x78000` depending on `R8_GLOB_CFG_INFO` bit 5
//! (`RB_BOOT_LOADER`).
//!
//! This module is deliberately separate from `bin/code.rs`: that algorithm's
//! `flash_limit()` is derived from `R8_CHIP_ID`, which is `0x92` on both CH584
//! and CH584X. CH584X is selected by the target YAML and verified here via
//! `(*(uint32_t *)0x7F024 & 0x0F) == 0x08`, the same discriminator `libISP585`
//! uses.

use core::ptr::{read_volatile, write_volatile};

const CTRL: usize = 0x4000_1806;
const DATA: usize = 0x4000_1804;
const WDATA: usize = 0x4000_1800;
const ROMCFG: usize = 0x4000_1044;
const GLOB_CFG_INFO: usize = 0x4000_1045;
const SAM: usize = 0x4000_1040;
/// `ROM_CFG_CHIP_ID`; the low nibble is `0x08` only on CH584X.
const ROM_CFG_CHIP_ID: usize = 0x0007_F024;

/// Datasheet CodeFlash end for CH584X (480 KiB).
const CODE_LIMIT: u32 = 0x0007_8000;
/// Conservative user-code end used by ISP585 while `RB_BOOT_LOADER` is set.
const CODE_LIMIT_BOOT: u32 = 0x0007_0000;
/// CH584X CodeFlash engine frame offset.
const ADDR_OFS: u32 = 0x0008_0000;
const ERASE_SIZE: u32 = 0x1000;

#[inline(always)]
unsafe fn r8(a: usize) -> u8 {
    unsafe { read_volatile(a as *const u8) }
}

#[inline(always)]
unsafe fn w8(a: usize, v: u8) {
    unsafe { write_volatile(a as *mut u8, v) }
}

#[inline(always)]
unsafe fn w32(a: usize, v: u32) {
    unsafe { write_volatile(a as *mut u32, v) }
}

#[inline(always)]
unsafe fn beg(c: u8) {
    unsafe {
        w8(CTRL, 0);
        w8(CTRL, 5);
        core::arch::asm!("nop; nop");
        w8(DATA, c);
    }
}

#[inline(always)]
unsafe fn end() {
    unsafe {
        while (r8(CTRL) as i8) < 0 {}
        w8(CTRL, 0);
    }
}

#[inline(always)]
unsafe fn r#in() -> u8 {
    unsafe {
        while (r8(CTRL) as i8) < 0 {}
        r8(DATA)
    }
}

#[inline(always)]
unsafe fn out(v: u8) {
    unsafe {
        while (r8(CTRL) as i8) < 0 {}
        w8(DATA, v);
    }
}

#[inline(always)]
unsafe fn addr(c: u8, mut a: u32) {
    unsafe {
        if (c & 0xbf) != 0x0b {
            beg(6);
            end();
        }
        beg(c);
        let mut i = 3;
        loop {
            out(((a >> 16) & 0xff) as u8);
            a <<= 8;
            i -= 1;
            if i == 0 {
                break;
            }
        }
    }
}

/// Start a read at `a`; the engine expects the five-byte read frame (three
/// address bytes plus two zero bytes), then returns one byte per `IN`.
#[inline(always)]
unsafe fn read_addr(c: u8, a: u32) {
    unsafe {
        beg(c);
        out((a >> 16) as u8);
        out((a >> 8) as u8);
        out(a as u8);
        out(0);
        out(0);
    }
}

#[inline(always)]
unsafe fn wait() -> u8 {
    unsafe {
        let mut n = 0x80000u32;
        loop {
            end();
            beg(5);
            let _ = r#in();
            let st = r#in();
            end();
            if (st & 1) == 0 {
                return st | 1;
            }
            n -= 1;
            if n == 0 {
                return 0;
            }
        }
    }
}

#[inline(always)]
unsafe fn write_dword(v: u32) {
    unsafe {
        w32(WDATA, v);
        let mut i = 4;
        loop {
            while (r8(CTRL) as i8) < 0 {}
            w8(CTRL, 0x15);
            i -= 1;
            if i == 0 {
                break;
            }
        }
    }
}

#[inline(always)]
unsafe fn unlock_and_reset() {
    unsafe {
        core::arch::asm!("csrci mstatus, 8");
        w8(SAM, 0x57);
        w8(SAM, 0xa8);
        w8(ROMCFG, r8(ROMCFG) | 0xe0);
        w8(SAM, 0x00);
        w8(CTRL, 0x04);
        beg(0xff);
        end();
    }
}

#[inline(always)]
unsafe fn lock() {
    unsafe {
        w8(SAM, 0x57);
        w8(SAM, 0xa8);
        w8(ROMCFG, r8(ROMCFG) & 0x10);
        w8(SAM, 0x00);
    }
}

/// The low nibble of `ROM_CFG_CHIP_ID` is `0x08` only on CH584X.
#[inline(always)]
fn is_584x() -> bool {
    unsafe { read_volatile(ROM_CFG_CHIP_ID as *const u32) & 0x0F == 0x08 }
}

/// `RB_BOOT_LOADER` (bit 5) makes ISP585 use the conservative 448 KiB limit.
#[inline(always)]
unsafe fn code_limit() -> u32 {
    if unsafe { r8(GLOB_CFG_INFO) } & 0x20 != 0 {
        CODE_LIMIT_BOOT
    } else {
        CODE_LIMIT
    }
}

/// Convert a CPU CodeFlash address to the engine's frame address.
#[inline(always)]
fn frame_addr(addr: u32) -> u32 {
    addr + ADDR_OFS
}

#[inline(always)]
unsafe fn erase_block(cmd: u8, at: u32) -> u32 {
    unsafe {
        unlock_and_reset();
        addr(cmd, frame_addr(at));
        let st = wait();
        lock();
        if st == 0 { 1 } else { 0 }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn init(addr: u32, _clk: u32, _fn_code: u32) -> u32 {
    if !is_584x() || addr >= unsafe { code_limit() } {
        1
    } else {
        0
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn uninit(_fn_code: u32) -> u32 {
    unsafe {
        w8(CTRL, 0);
        0
    }
}

/// Erase the 4 KiB CodeFlash block containing `sector_addr`.
#[unsafe(no_mangle)]
pub extern "C" fn erase_sector(sector_addr: u32) -> u32 {
    if !is_584x() {
        return 1;
    }
    let aligned = sector_addr & !(ERASE_SIZE - 1);
    let limit = unsafe { code_limit() };
    if aligned >= limit || limit - aligned < ERASE_SIZE {
        return 1;
    }
    unsafe { erase_block(0x20, aligned) }
}

/// Erase the whole user CodeFlash: 64 KiB `0xD8` blocks, then 4 KiB `0x20`
/// sectors for the tail (480 KiB leaves a 32 KiB tail).
#[unsafe(no_mangle)]
pub extern "C" fn erase_all() -> u32 {
    if !is_584x() {
        return 1;
    }
    let limit = unsafe { code_limit() };
    let mut a = 0u32;
    while a < limit {
        let (cmd, step) = if a & 0xffff == 0 && limit - a >= 0x1_0000 {
            (0xd8, 0x1_0000)
        } else {
            (0x20, ERASE_SIZE)
        };
        let r = unsafe { erase_block(cmd, a) };
        if r != 0 {
            return r;
        }
        a += step;
    }
    0
}

/// Program `size` bytes at CPU CodeFlash address `dest`.
///
/// The engine takes `dest + 0x80000`; programming is dword-wise `0x02` with
/// four `CTRL=0x15` pulses per dword, re-issuing the command every 256 bytes.
#[unsafe(no_mangle)]
pub extern "C" fn program_page(dest: u32, size: u32, buf: *const u8) -> u32 {
    if size == 0 {
        return 0;
    }
    if !is_584x() {
        return 1;
    }
    let limit = unsafe { code_limit() };
    if dest >= limit || limit - dest < size || ((dest | size) & 3) != 0 {
        return 1;
    }
    unsafe {
        unlock_and_reset();
        let mut offset = 0u32;
        let mut ok = true;
        while offset < size {
            let chunk = if size - offset > 0x100 {
                0x100
            } else {
                size - offset
            };
            addr(0x02, frame_addr(dest + offset));
            let mut i = 0u32;
            while i < chunk {
                let p = buf.add((offset + i) as usize);
                let v = u32::from_le_bytes([*p, *p.add(1), *p.add(2), *p.add(3)]);
                write_dword(v);
                i += 4;
            }
            if wait() == 0 {
                ok = false;
                break;
            }
            offset += chunk;
        }
        lock();
        if ok { 0 } else { 1 }
    }
}

/// Read `size` bytes at CPU CodeFlash address `addr` through the flash engine.
///
/// A halted debug read of CodeFlash returns the `0xF3F9BDA9` placeholder, so
/// this entry point is required for probe-rs `--verify`. The frame address is
/// `addr + 0x80000`; the read itself is byte-wise `0x0B`.
#[unsafe(no_mangle)]
pub extern "C" fn read_flash(addr: u32, size: u32, buf: *mut u8) -> u32 {
    if size == 0 {
        return 0;
    }
    if !is_584x() {
        return 1;
    }
    let limit = unsafe { code_limit() };
    if addr >= limit || limit - addr < size {
        return 1;
    }
    unsafe {
        unlock_and_reset();
        read_addr(0x0b, frame_addr(addr));
        let mut i = 0u32;
        while i < size {
            *buf.add(i as usize) = r#in();
            i += 1;
        }
        end();
        lock();
        0
    }
}
