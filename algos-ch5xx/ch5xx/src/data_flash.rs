//! DataFlash (EEPROM) support for the CH5xx serial flash engine.
//!
//! This module is shared by two `[[bin]]` targets. They differ only in the
//! address bit the engine expects for DataFlash:
//!
//! - `data`      (`OR_MASK = 0`): frame address is `0x70000 + off`.
//!   Used by CH571/CH573 (ISP573), CH581/CH584/CH585 (ISP585).
//! - `data-bank` (`OR_MASK = 0x80000`): frame address is `0xF0000 + off`.
//!   Used by CH582/CH583 (ISP583), CH591/CH592 (ISP592).
//!
//! The two variants cannot be merged into one binary and dispatched at runtime
//! by `R8_CHIP_ID`: `0x92` is CH581/CH584 (no bank bit) as well as CH592 (bank
//! bit), so the id does not determine the frame address. probe-rs therefore
//! selects the binary statically per chip in the target YAML.
//!
//! The protocol is not a CodeFlash alias: DataFlash is erased in
//! 256-byte sectors (`0x81`) and programmed byte-wise (`0x02`). Reads must go
//! through the engine (`0x0B`) because direct bus reads return the sentinel
//! `0xF3F9BDA9`. See `research/ch585-ch584-flash-partition-baseline.md` for the
//! reverse engineering and board measurements.

use core::ptr::{read_volatile, write_volatile};

/// DataFlash window in the CODE address space.
const DATA_START: u32 = 0x0007_0000;
const DATA_END: u32 = 0x0007_8000;

/// Bit OR-ed into the engine frame address. `0` for ISP573/ISP585 parts,
/// `0x80000` for ISP583/ISP592 parts.
const OR_MASK: u32 = crate::OR_MASK;

const CTRL: usize = 0x4000_1806;
const DATA: usize = 0x4000_1804;
const ROMCFG: usize = 0x4000_1044;
const SAM: usize = 0x4000_1040;

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

#[inline(always)]
fn in_range(addr: u32, size: u32) -> bool {
    addr >= DATA_START && addr < DATA_END && size <= DATA_END - addr
}

#[inline(always)]
unsafe fn erase_block(cmd: u8, at: u32) -> u32 {
    unsafe {
        unlock_and_reset();
        addr(cmd, at | OR_MASK);
        let st = wait();
        lock();
        if st == 0 { 1 } else { 0 }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn init(addr: u32, _clk: u32, _op: u32) -> u32 {
    if addr == DATA_START { 0 } else { 1 }
}

#[unsafe(no_mangle)]
pub extern "C" fn uninit(_op: u32) -> u32 {
    unsafe {
        w8(CTRL, 0);
        0
    }
}

/// Erase the 256-byte DataFlash sector containing `sector_addr`.
#[unsafe(no_mangle)]
pub extern "C" fn erase_sector(sector_addr: u32) -> u32 {
    let aligned = sector_addr & !0xff;
    if !in_range(aligned, 0x100) {
        return 1;
    }
    unsafe { erase_block(0x81, aligned) }
}

/// Erase the whole 32 KiB DataFlash. `0x20` is the 4 KiB erase command; the ISP
/// library uses it for aligned DataFlash ranges. Kept as a standalone entry
/// point but not advertised as `pc_erase_all` in the target YAML, so a
/// `probe-rs erase` mass erase never wipes BLE bonding data implicitly.
#[unsafe(no_mangle)]
pub extern "C" fn erase_all() -> u32 {
    let mut a = DATA_START;
    while a < DATA_END {
        let r = unsafe { erase_block(0x20, a) };
        if r != 0 {
            return r;
        }
        a += 0x1000;
    }
    0
}

/// Program `size` bytes at absolute DataFlash address `dest`.
///
/// DataFlash is programmed byte-wise and without the CodeFlash `CTRL=0x15`
/// pulses; each 256-byte page is a separate `0x02` command.
#[unsafe(no_mangle)]
pub extern "C" fn program_page(dest: u32, size: u32, buf: *const u8) -> u32 {
    if size == 0 {
        return 0;
    }
    if !in_range(dest, size) {
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
            addr(0x02, (dest + offset) | OR_MASK);
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

/// Read `size` bytes at `addr` into `buf` through the flash engine.
///
/// probe-rs calls this as `ReadFlash(adr: u32, sz: u32, buf: *mut u8)`, one
/// `page_size` chunk at a time. Direct bus reads of DataFlash return
/// `0xF3F9BDA9`, so a `pc_read` entry point is required for `--verify`.
#[unsafe(no_mangle)]
pub extern "C" fn read_flash(addr: u32, size: u32, buf: *mut u8) -> u32 {
    if size == 0 {
        return 0;
    }
    if !in_range(addr, size) {
        return 1;
    }
    unsafe {
        unlock_and_reset();
        read_addr(0x0b, addr | OR_MASK);
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
