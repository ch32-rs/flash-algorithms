//! DataFlash (EEPROM) support for CH565/CH569.
//!
//! The 32 KiB DataFlash occupies `0x70000..0x78000` and shares the SYS-block
//! flash window at `0x40001014`/`18`/`1a` with CodeFlash. Unlike CodeFlash,
//! it is erased in 4 KiB blocks with command `0x20`, programmed byte-wise with
//! `0x02` (no `CTRL=0x15` dword pulses), and read with command `0x0B`.
//!
//! The engine takes absolute addresses here, just as the CodeFlash algorithm
//! does; there is no `0x80000` bank bit on CH565/CH569.

use flash_algorithm::{ErrorCode, FlashAlgorithm, Function, algorithm};

const CTRL: usize = 0x4000_101a;
const DATA: usize = 0x4000_1018;
const WDATA: usize = 0x4000_1014;
const SAM: usize = 0x4000_1000;
const ROMCFG: usize = 0x4000_1004;

/// DataFlash range. CodeFlash ends at `DATA_START`.
const DATA_START: u32 = 0x0007_0000;
const DATA_END: u32 = 0x0007_8000;
const ERASE_SIZE: u32 = 0x1000;
/// Bytes issued under one program command (`0x02`) before the status poll.
const PAGE_SIZE: u32 = 0x100;
const PROGRAM_TIMEOUT_MS: u32 = 500;
const ERASE_TIMEOUT_MS: u32 = 1000;

/// `RB_ROM_CODE_WE | RB_ROM_DATA_WE | RB_CODE_RAM_WE` = bits 3, 2, 1.
const ROM_WE: u8 = 0x8c;
const ROM_LOCK_KEEP: u8 = 0x80;
const ROM_LOCK_MASK: u8 = 0x73;

struct Algo;

algorithm!(Algo, {
    device_name: "CH565/CH569 DataFlash",
    device_type: DeviceType::Onchip,
    flash_address: DATA_START,
    flash_size: DATA_END - DATA_START,
    page_size: PAGE_SIZE,
    empty_value: 0xFF,
    program_time_out: PROGRAM_TIMEOUT_MS,
    erase_time_out: ERASE_TIMEOUT_MS,
    sectors: [{
        size: ERASE_SIZE,
        address: 0,
    }]
});

impl FlashAlgorithm for Algo {
    fn new(addr: u32, _clock: u32, _function: Function) -> Result<Self, ErrorCode> {
        if addr != DATA_START {
            return Err(ErrorCode::new(1).unwrap());
        }
        Ok(Self)
    }

    fn erase_all(&mut self) -> Result<(), ErrorCode> {
        let mut a = DATA_START;
        while a < DATA_END {
            self.erase_block(0x20, a)?;
            a += ERASE_SIZE;
        }
        Ok(())
    }

    fn erase_sector(&mut self, sector_addr: u32) -> Result<(), ErrorCode> {
        let aligned = sector_addr & !(ERASE_SIZE - 1);
        if aligned < DATA_START || DATA_END - aligned < ERASE_SIZE {
            return Err(ErrorCode::new(1).unwrap());
        }
        self.erase_block(0x20, aligned)
    }

    fn program_page(&mut self, dest: u32, data: &[u8]) -> Result<(), ErrorCode> {
        let size = data.len() as u32;
        if size == 0 {
            return Ok(());
        }
        if dest < DATA_START || DATA_END - dest < size {
            return Err(ErrorCode::new(1).unwrap());
        }
        unsafe {
            unlock();
            let mut offset = 0u32;
            let mut ok = true;
            while offset < size {
                let to_boundary = PAGE_SIZE - ((dest + offset) & (PAGE_SIZE - 1));
                let chunk = if size - offset > to_boundary {
                    to_boundary
                } else {
                    size - offset
                };
                addr(0x02, dest + offset);
                let mut i = 0u32;
                while i < chunk {
                    out(data[(offset + i) as usize]);
                    i += 1;
                }
                if wait() == 0 {
                    ok = false;
                    break;
                }
                offset += chunk;
            }
            lock();
            if ok {
                Ok(())
            } else {
                Err(ErrorCode::new(1).unwrap())
            }
        }
    }

    /// Read DataFlash through the engine. Direct debug reads of the flash
    /// window return the `0xF3F9BDA9` placeholder, so a `pc_read` entry point
    /// is required for probe-rs `--verify`.
    fn read_flash(&mut self, address: u32, data: &mut [u8]) -> Result<(), ErrorCode> {
        let size = data.len() as u32;
        if size == 0 {
            return Ok(());
        }
        if address < DATA_START || DATA_END - address < size || (size & 3) != 0 {
            return Err(ErrorCode::new(1).unwrap());
        }
        unsafe {
            let mut offset = 0u32;
            while offset < size {
                let chunk = if size - offset > PAGE_SIZE {
                    PAGE_SIZE
                } else {
                    size - offset
                };
                addr(0x0b, address + offset);
                // The engine's read frame leaves two bytes before the data.
                let _ = r#in();
                let _ = r#in();
                let mut i = 0u32;
                while i < chunk {
                    data[(offset + i) as usize..][..4].copy_from_slice(&word_in().to_le_bytes());
                    i += 4;
                }
                end();
                offset += chunk;
            }
        }
        Ok(())
    }
}

impl Algo {
    fn erase_block(&mut self, cmd: u8, addr_at: u32) -> Result<(), ErrorCode> {
        unsafe {
            unlock();
            addr(cmd, addr_at);
            let st = wait();
            lock();
            if st == 0 {
                Err(ErrorCode::new(1).unwrap())
            } else {
                Ok(())
            }
        }
    }
}

impl Drop for Algo {
    fn drop(&mut self) {
        unsafe { w8(CTRL, 0) }
    }
}

#[inline(always)]
unsafe fn r8(a: usize) -> u8 {
    unsafe { core::ptr::read_volatile(a as *const u8) }
}

#[inline(always)]
unsafe fn w8(a: usize, v: u8) {
    unsafe { core::ptr::write_volatile(a as *mut u8, v) }
}

/// CH565/CH569 start a command with CTRL `7`, not `5` as CH57x/CH58x/CH59x.
#[inline(always)]
unsafe fn beg(c: u8) {
    unsafe {
        w8(CTRL, 0);
        w8(CTRL, 7);
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

/// Send a 24-bit address command. `0x0B` (read) skips the `6` preamble; every
/// other command sends it first, matching the official loader.
#[inline(always)]
unsafe fn addr(c: u8, mut a: u32) {
    unsafe {
        if c != 0x0b {
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

/// Read one 32-bit word. The data register only clocks the transfer; the
/// latched word is read from `WDATA`, as the official loader does.
#[inline(always)]
unsafe fn word_in() -> u32 {
    unsafe {
        let _ = r#in();
        let _ = r#in();
        let _ = r#in();
        let _ = r#in();
        core::ptr::read_volatile(WDATA as *const u32)
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
unsafe fn unlock() {
    unsafe {
        core::arch::asm!("csrci mstatus, 8");
        w8(SAM, 0x57);
        w8(SAM, 0xa8);
        w8(ROMCFG, r8(ROMCFG) | ROM_WE);
    }
}

#[inline(always)]
unsafe fn lock() {
    unsafe {
        w8(SAM, 0x57);
        w8(SAM, 0xa8);
        w8(ROMCFG, (r8(ROMCFG) & ROM_LOCK_MASK) | ROM_LOCK_KEEP);
    }
}
