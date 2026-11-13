//! DataFlash (EEPROM) support for CH584X.
//!
//! CH584X has 512 KiB of DataFlash but no CPU-mapped address for it. The
//! ISP585 EEPROM path addresses it 0-based (`0x00000..0x7FFFF`) and erases in
//! 4 KiB blocks with command `0x20`; programming is byte-wise `0x02` and reads
//! go through the engine (`0x0B`).
//!
//! probe-rs exposes the region at the synthetic program-view address
//! `0x80000..0x100000`, matching WCH's `FLASH_ROM_*` `+0x80000` convention.
//! [`frame_addr`] subtracts that base before talking to the engine.
//!
//! This module is deliberately separate from `data_flash.rs`: the
//! CH571/573/581/582/583/584/585/591/592 binaries are board-tested and keep
//! their 256-byte erase / `0x70000` frame address.

use core::ptr::{read_volatile, write_volatile};

const DATA_START: u32 = 0x0008_0000;
const DATA_END: u32 = 0x0010_0000;
const ADDR_SUB: u32 = 0x0008_0000;
const ERASE_CMD: u8 = 0x20;
const ERASE_SIZE: u32 = 0x1000;

const CTRL: usize = 0x4000_1806;
const DATA: usize = 0x4000_1804;
const ROMCFG: usize = 0x4000_1044;
const SAM: usize = 0x4000_1040;
/// `ROM_CFG_CHIP_ID`; the low nibble is `0x08` only on CH584X.
const ROM_CFG_CHIP_ID: usize = 0x0007_F024;

#[inline(always)]
unsafe fn r8(a: usize) -> u8 {
    unsafe { read_volatile(a as *const u8) }
}

#[inline(always)]
unsafe fn w8(a: usize, v: u8) {
    unsafe { write_volatile(a as *mut u8, v) }
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

/// Start a non-read command at `a`; the engine takes three address bytes.
#[inline(always)]
unsafe fn addr(c: u8, a: u32) {
    unsafe {
        beg(6);
        end();
        beg(c);
        out((a >> 16) as u8);
        out((a >> 8) as u8);
        out(a as u8);
    }
}

/// Start a read command at `a`; the engine expects the five-byte read frame
/// (three address bytes plus two zero bytes).
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

/// Poll the engine status until it reports completion.
///
/// Returns `st | 1` on success and `0` on timeout, matching the CodeFlash
/// algorithm's convention.
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

/// The low nibble of `ROM_CFG_CHIP_ID` is `0x08` only on CH584X. This is the
/// same discriminator `libISP585` uses; `R8_CHIP_ID` and the WCH-Link device id
/// are shared with CH584.
#[inline(always)]
fn is_584x() -> bool {
    unsafe { read_volatile(ROM_CFG_CHIP_ID as *const u32) & 0x0F == 0x08 }
}

/// Convert probe-rs's program-view address to the engine's 0-based DataFlash
/// frame address.
#[inline(always)]
fn frame_addr(addr: u32) -> u32 {
    addr - ADDR_SUB
}

#[inline(always)]
fn in_range(addr: u32, size: u32) -> bool {
    addr >= DATA_START && addr < DATA_END && size <= DATA_END - addr
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
pub extern "C" fn init(addr: u32, _clk: u32, _op: u32) -> u32 {
    if !is_584x() || addr != DATA_START {
        1
    } else {
        0
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn uninit(_op: u32) -> u32 {
    unsafe {
        w8(CTRL, 0);
        0
    }
}

/// Erase the 4 KiB DataFlash block containing `sector_addr`.
#[unsafe(no_mangle)]
pub extern "C" fn erase_sector(sector_addr: u32) -> u32 {
    if !is_584x() {
        return 1;
    }
    let aligned = sector_addr & !(ERASE_SIZE - 1);
    if !in_range(aligned, ERASE_SIZE) {
        return 1;
    }
    unsafe { erase_block(ERASE_CMD, aligned) }
}

/// Erase the whole 512 KiB DataFlash. Kept as a standalone entry point but not
/// advertised as `pc_erase_all` in the target YAML, so a `probe-rs erase` mass
/// erase never wipes DataFlash implicitly.
#[unsafe(no_mangle)]
pub extern "C" fn erase_all() -> u32 {
    if !is_584x() {
        return 1;
    }
    let mut a = DATA_START;
    while a < DATA_END {
        let r = unsafe { erase_block(ERASE_CMD, a) };
        if r != 0 {
            return r;
        }
        a += ERASE_SIZE;
    }
    0
}

/// Program `size` bytes at program-view address `dest`.
///
/// DataFlash is programmed byte-wise and without the CodeFlash `CTRL=0x15`
/// pulses; each 256-byte page is a separate `0x02` command.
#[unsafe(no_mangle)]
pub extern "C" fn program_page(dest: u32, size: u32, buf: *const u8) -> u32 {
    if size == 0 {
        return 0;
    }
    if !is_584x() || !in_range(dest, size) {
        return 1;
    }
    unsafe {
        unlock_and_reset();
        let mut offset = 0u32;
        let mut ok = true;
        while offset < size {
            let to_boundary = 0x100 - ((dest + offset) & 0xff);
            let chunk = if size - offset > to_boundary {
                to_boundary
            } else {
                size - offset
            };
            addr(0x02, frame_addr(dest + offset));
            let mut i = 0u32;
            while i < chunk {
                out(*buf.add((offset + i) as usize));
                i += 1;
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

/// Read `size` bytes at program-view address `addr` into `buf` through the
/// flash engine.
///
/// probe-rs calls this as `ReadFlash(adr: u32, sz: u32, buf: *mut u8)`, one
/// `page_size` chunk at a time. Direct bus reads of DataFlash return
/// `0xF3F9BDA9`, so a `pc_read` entry point is required for `--verify`.
#[unsafe(no_mangle)]
pub extern "C" fn read_flash(addr: u32, size: u32, buf: *mut u8) -> u32 {
    if size == 0 {
        return 0;
    }
    if !is_584x() || !in_range(addr, size) {
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
