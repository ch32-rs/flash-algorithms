//! Low-level CH595/CH596 page-latch flash controller.
//!
//! CH595 and CH596 drop the serial flash window used by the rest of the
//! CH5xx family and expose a hardware state machine at
//! `0x4000_1800..0x4000_1820`. The sequence follows the public
//! `EVT/EXAM/SRC/StdPeriphDriver/CH59x_flash.c`:
//!
//! - unlock with the `0x57`/`0xA8` safe-access signature, then write
//!   `R32_GLOBAL_CONFIG = 0xE1` to open the erase/program channel;
//! - pre-program (`R8_PREPG_CTRL`) and erase (`R8_ERASE_CTRL`) the range;
//! - clear the 256-byte page latch (`R8_BUF_CTRL` bit 0), load it through
//!   `R32_FLASH_DATA` with address auto-increment, then start page program
//!   (`R8_PROGRAM_CTRL`);
//! - poll `R32_FLASH_STATUS`: bit 31 is busy, bit 19 is ready.
//!
//! Reads are plain memory-mapped loads; the controller is not involved.

use core::ptr::{read_volatile, write_volatile};

const PREPG_CTRL: usize = 0x4000_1800;
const BUF_CTRL: usize = 0x4000_1801;
const ERASE_CTRL: usize = 0x4000_1802;
const PROGRAM_CTRL: usize = 0x4000_1803;
const READ_CTRL: usize = 0x4000_1807;
const FLASH_ADDR: usize = 0x4000_180c;
const FLASH_DATA: usize = 0x4000_1810;
const FLASH_STATUS: usize = 0x4000_1820;

const SLP_WAKE_CFG: usize = 0x4000_1016;
const SAFE_ACCESS_SIG: usize = 0x4000_1040;
const CHIP_ID: usize = 0x4000_1041;
const GLOBAL_CONFIG: usize = 0x4000_1044;

/// `ROM_CFG_ADDR` in `CH59x_flash.h`. Writing it after an operation is the
/// official "operate flash once to restore it" workaround.
const ROM_CFG_ADDR: usize = 0x0004_0200;

const SIG1: u8 = 0x57;
const SIG2: u8 = 0xa8;

const ACAUTO_ENABLE: u16 = 0x100;
const STATUS_BUSY: u32 = 1 << 31;
const STATUS_READY: u32 = 1 << 19;

pub const PAGE_SIZE: u32 = 0x100;
pub const SECTOR_SIZE: u32 = 0x800;
pub const BANK_SIZE: u32 = 0x1_0000;

/// The CH59x driver polls without a bound; keep one so a stuck controller
/// returns an error instead of hanging the probe.
const WAIT_LIMIT: u32 = 1_000_000;

const PREPG_PAGE: u8 = 0;
const PREPG_SECTOR: u8 = 1 << 1;
const PREPG_BANK: u8 = 2 << 1;
const PREPG_START: u8 = 1 << 4;

const ERASE_PAGE: u8 = 0;
const ERASE_SECTOR: u8 = 1 << 0;
const ERASE_BANK: u8 = 1 << 1;
const ERASE_START: u8 = 1 << 4;

const BUF_CLEAR: u8 = 1 << 0;
const BUF_LOAD_AUTO: u8 = 1 << 5;

/// `RB_FLASH_CLK = 11` (two system clocks high) plus address auto-increment.
const READ_CTRL_WRITE: u8 = (3 << 4) | (1 << 2);

#[inline(always)]
unsafe fn r8(a: usize) -> u8 {
    unsafe { read_volatile(a as *const u8) }
}

#[inline(always)]
unsafe fn w8(a: usize, v: u8) {
    unsafe { write_volatile(a as *mut u8, v) }
}

#[inline(always)]
unsafe fn r16(a: usize) -> u16 {
    unsafe { read_volatile(a as *const u16) }
}

#[inline(always)]
unsafe fn w16(a: usize, v: u16) {
    unsafe { write_volatile(a as *mut u16, v) }
}

#[inline(always)]
unsafe fn r32(a: usize) -> u32 {
    unsafe { read_volatile(a as *const u32) }
}

#[inline(always)]
unsafe fn w32(a: usize, v: u32) {
    unsafe { write_volatile(a as *mut u32, v) }
}

#[inline(always)]
unsafe fn unlock() {
    unsafe {
        // The safe-access window is only ~112 cycles; keep interrupts out of
        // the signature sequence.
        core::arch::asm!("csrci mstatus, 8");
        w8(SAFE_ACCESS_SIG, SIG1);
        w8(SAFE_ACCESS_SIG, SIG2);
    }
}

#[inline(always)]
unsafe fn lock() {
    unsafe { w8(SAFE_ACCESS_SIG, 0) }
}

/// Wait until the controller clears `RB_CTRL_BUSY` and raises
/// `RB_FLASH_READY`.
#[inline(always)]
unsafe fn ready() -> bool {
    unsafe {
        let mut n = 0u32;
        while r32(FLASH_STATUS) & STATUS_BUSY != 0 {
            n += 1;
            if n > WAIT_LIMIT {
                return false;
            }
        }
        n = 0;
        while r32(FLASH_STATUS) & STATUS_READY == 0 {
            n += 1;
            if n > WAIT_LIMIT {
                return false;
            }
        }
        true
    }
}

/// Open the erase/program channel and turn off safe-access auto-close so it
/// stays open across the long pre-program/erase/program waits.
unsafe fn open() {
    unsafe {
        unlock();
        w16(SLP_WAKE_CFG, r16(SLP_WAKE_CFG) & !ACAUTO_ENABLE);
        w32(GLOBAL_CONFIG, 0xe1);
        w8(READ_CTRL, 0);
        w8(READ_CTRL, 3 << 4);
    }
}

unsafe fn close() {
    unsafe {
        w16(SLP_WAKE_CFG, r16(SLP_WAKE_CFG) | ACAUTO_ENABLE);
        lock();
        // Official "operate flash once to restore it" access.
        w32(ROM_CFG_ADDR, 0);
    }
}

/// Run pre-program followed by erase with `mode`.
unsafe fn erase_mode(addr: u32, prepg_mode: u8, erase_mode: u8) -> bool {
    unsafe {
        open();
        w32(FLASH_ADDR, addr);
        w8(PREPG_CTRL, prepg_mode);
        w8(PREPG_CTRL, prepg_mode | PREPG_START);
        let ok = ready()
            && {
                w8(ERASE_CTRL, erase_mode | ERASE_START);
                ready()
            }
            && {
                w8(ERASE_CTRL, 0);
                true
            };
        close();
        ok
    }
}

pub unsafe fn erase_page(addr: u32) -> bool {
    unsafe { erase_mode(addr, PREPG_PAGE, ERASE_PAGE) }
}

pub unsafe fn erase_sector(addr: u32) -> bool {
    unsafe { erase_mode(addr, PREPG_SECTOR, ERASE_SECTOR) }
}

pub unsafe fn erase_bank(addr: u32) -> bool {
    unsafe { erase_mode(addr, PREPG_BANK, ERASE_BANK) }
}

/// Erase `[start, end)`, preferring 64 KiB banks, then 2 KiB sectors, then
/// 256-byte pages for an unaligned tail. Mirrors `FLASH_ERASE`.
pub unsafe fn erase_range(start: u32, end: u32) -> bool {
    unsafe {
        let mut addr = start;
        while addr < end {
            let remaining = end - addr;
            if addr % BANK_SIZE == 0 && remaining >= BANK_SIZE {
                if !erase_bank(addr) {
                    return false;
                }
                addr += BANK_SIZE;
            } else if addr % SECTOR_SIZE == 0 && remaining >= SECTOR_SIZE {
                if !erase_sector(addr) {
                    return false;
                }
                addr += SECTOR_SIZE;
            } else {
                let aligned = addr & !(PAGE_SIZE - 1);
                if !erase_page(aligned) {
                    return false;
                }
                addr = aligned + PAGE_SIZE;
            }
        }
        true
    }
}

/// Program `len` bytes from `buf` at `addr`; every 256-byte page is latched
/// and programmed separately, as in `FLASH_WRITE`.
pub unsafe fn write(addr: u32, buf: *const u8, len: u32) -> bool {
    unsafe {
        if len == 0 {
            return true;
        }
        open();
        let mut offset = 0u32;
        while offset < len {
            // A latch load must not cross a page: the address auto-increments
            // only within `R32_FLASH_ADDR[7:2]`.
            let page_remaining = PAGE_SIZE - ((addr + offset) & (PAGE_SIZE - 1));
            let chunk = core::cmp::min(len - offset, page_remaining);

            w8(BUF_CTRL, BUF_CLEAR);
            if !ready() {
                close();
                return false;
            }

            w8(READ_CTRL, READ_CTRL_WRITE);
            w8(BUF_CTRL, BUF_LOAD_AUTO);
            w32(FLASH_ADDR, addr + offset);

            // The latch accepts 32-bit words. Pad a short tail with erased
            // value so the final word can be loaded without reading past the
            // caller's buffer.
            let words = chunk.div_ceil(4);
            let mut i = 0u32;
            while i < words {
                let mut value = 0u32;
                let mut b = 0u32;
                while b < 4 {
                    let index = i * 4 + b;
                    let byte = if index < chunk {
                        *buf.add((offset + index) as usize)
                    } else {
                        0xff
                    };
                    value |= (byte as u32) << (8 * b);
                    b += 1;
                }
                if !ready() {
                    close();
                    return false;
                }
                w32(FLASH_DATA, value);
                i += 1;
            }

            if !ready() {
                close();
                return false;
            }
            w32(FLASH_ADDR, addr + offset);
            w8(PROGRAM_CTRL, 1);
            if !ready() {
                close();
                return false;
            }
            offset += chunk;
        }
        close();
        true
    }
}

/// Memory-mapped read; `FLASH_READ` in the official driver is a byte loop.
pub unsafe fn read(addr: u32, buf: *mut u8, len: u32) -> bool {
    unsafe {
        let mut i = 0u32;
        while i < len {
            *buf.add(i as usize) = r8((addr + i) as usize);
            i += 1;
        }
        true
    }
}

/// `R8_CHIP_ID`: 0x97 for CH595, 0x96 for CH596.
pub unsafe fn chip_id() -> u8 {
    unsafe { r8(CHIP_ID) }
}
