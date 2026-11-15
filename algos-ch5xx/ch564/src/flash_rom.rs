//! Low-level CH564 flash controller.
//!
//! CH564 moves the flash window to `0x40400018` (DATA) / `0x4040001a` (CTRL),
//! separate from both the CH56x SYS block and the CH57x/CH58x/CH59x
//! `0x40001800` window. The protocol is derived from the official
//! `libISP564.a`:
//!
//! - erase/program: `CTRL=3`, send the `6` prefix, `CTRL=0`, `CTRL=3`, then
//!   the command byte and three address bytes;
//! - read: `CTRL=7`, then the command byte and three address bytes. The
//!   controller appends the two dummy bytes, so the reader discards two
//!   `DATA` reads before consuming payload bytes;
//! - status: `CTRL=7`, `DATA=5`, read two bytes, and check bit 0 of the second;
//! - every DATA/CTRL access is followed by the library's
//!   `DelayForSPIGeneration` (18 NOPs).

use core::ptr::{read_volatile, write_volatile};

const DATA: usize = 0x4040_0018;
const CTRL: usize = 0x4040_001a;

const CTRL_READ: u8 = 7;
const CTRL_WRITE: u8 = 3;
/// `FLASH_ROM_WAIT` in `libISP564.a` gives up after 0x3a97f polls.
const WAIT_LIMIT: u32 = 0x3a97f;
const PAGE: u32 = 0x100;

const CMD_READ: u8 = 0x0b;
const CMD_SECURITY_READ: u8 = 0x48;
const CMD_PROGRAM: u8 = 0x02;
const CMD_ERASE_4K: u8 = 0x20;
const CMD_ERASE_64K: u8 = 0xd8;

#[inline(always)]
unsafe fn r8(a: usize) -> u8 {
    unsafe { read_volatile(a as *const u8) }
}

#[inline(always)]
unsafe fn w8(a: usize, v: u8) {
    unsafe { write_volatile(a as *mut u8, v) }
}

/// `DelayForSPIGeneration`: 18 NOPs after every controller access.
#[inline(always)]
unsafe fn delay() {
    unsafe {
        core::arch::asm!(
            "nop; nop; nop; nop; nop; nop; nop; nop; nop",
            "nop; nop; nop; nop; nop; nop; nop; nop; nop",
            options(nomem, nostack),
        )
    }
}

/// Poll the flash status command until bit 0 clears.
#[inline(always)]
unsafe fn wait() -> bool {
    unsafe {
        w8(CTRL, 0);
        delay();
        let mut n = 0u32;
        loop {
            if n > WAIT_LIMIT {
                return false;
            }
            w8(CTRL, CTRL_READ);
            delay();
            w8(DATA, 5);
            delay();
            let _ = r8(DATA);
            delay();
            let st = r8(DATA);
            delay();
            w8(CTRL, 0);
            if st & 1 == 0 {
                return true;
            }
            n += 1;
        }
    }
}

/// Start `cmd` at `addr`. Read commands use the `CTRL=7` frame; all other
/// commands first send the library's `6` prefix under `CTRL=3`.
#[inline(always)]
unsafe fn cmd(c: u8, addr: u32) -> bool {
    unsafe {
        if !wait() {
            return false;
        }
        if c == CMD_READ || c == CMD_SECURITY_READ {
            w8(CTRL, CTRL_READ);
            delay();
        } else {
            w8(CTRL, CTRL_WRITE);
            delay();
            w8(DATA, 6);
            delay();
            w8(CTRL, 0);
            delay();
            w8(CTRL, CTRL_WRITE);
            delay();
        }
        w8(DATA, c);
        delay();
        w8(DATA, (addr >> 16) as u8);
        delay();
        w8(DATA, (addr >> 8) as u8);
        delay();
        w8(DATA, addr as u8);
        delay();
        true
    }
}

/// Read `len` bytes from `addr` with command `0x0B`.
pub unsafe fn read(addr: u32, buf: *mut u8, len: u32) -> bool {
    unsafe {
        if !cmd(CMD_READ, addr) {
            return false;
        }
        // The controller appends two dummy bytes to the three-byte address.
        let _ = r8(DATA);
        delay();
        let _ = r8(DATA);
        delay();
        let mut i = 0u32;
        while i < len {
            *buf.add(i as usize) = r8(DATA);
            delay();
            i += 1;
        }
        wait()
    }
}

/// Read through the security/InfoFlash command `0x48`.
pub unsafe fn security_read(addr: u32, buf: *mut u8, len: u32) -> bool {
    unsafe {
        if !cmd(CMD_SECURITY_READ, addr) {
            return false;
        }
        let _ = r8(DATA);
        delay();
        let _ = r8(DATA);
        delay();
        let mut i = 0u32;
        while i < len {
            *buf.add(i as usize) = r8(DATA);
            delay();
            i += 1;
        }
        wait()
    }
}

/// Program `len` bytes at `addr`, re-issuing command `0x02` at every 256-byte
/// boundary. DataFlash and CodeFlash share this byte-wise path.
pub unsafe fn write(addr: u32, buf: *const u8, len: u32) -> bool {
    unsafe {
        if len == 0 {
            return true;
        }
        let mut offset = 0u32;
        while offset < len {
            let to_boundary = PAGE - ((addr + offset) & (PAGE - 1));
            let chunk = if len - offset > to_boundary {
                to_boundary
            } else {
                len - offset
            };
            if !cmd(CMD_PROGRAM, addr + offset) {
                return false;
            }
            let mut i = 0u32;
            while i < chunk {
                w8(DATA, *buf.add((offset + i) as usize));
                delay();
                i += 1;
            }
            offset += chunk;
        }
        wait()
    }
}

/// Erase one 4 KiB sector.
pub unsafe fn erase_sector(addr: u32) -> bool {
    unsafe { cmd(CMD_ERASE_4K, addr) && wait() }
}

/// Erase `[start, end)`, using 64 KiB blocks where they fit and 4 KiB sectors
/// for the tail.
pub unsafe fn erase_range(start: u32, end: u32) -> bool {
    unsafe {
        let mut a = start;
        while a < end {
            let (c, step) = if a & 0xffff == 0 && end - a >= 0x1_0000 {
                (CMD_ERASE_64K, 0x1_0000)
            } else {
                (CMD_ERASE_4K, 0x1000)
            };
            if !cmd(c, a) {
                return false;
            }
            a += step;
        }
        wait()
    }
}

/// The official `Get_Flash_Size` reads InfoFlash `0x3007` and tests bit 5:
/// 0 -> 192K CodeFlash + 32K DataFlash @ 0x30000, 1 -> 448K + 32K @ 0x70000.
pub unsafe fn config_512k() -> Option<bool> {
    let mut b = 0u8;
    if unsafe { security_read(0x3007, &mut b, 1) } {
        Some(b & 0x20 != 0)
    } else {
        None
    }
}
