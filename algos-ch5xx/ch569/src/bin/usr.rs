#![no_std]
#![no_main]

use flash_algorithm::{ErrorCode, FlashAlgorithm, Function, algorithm};

/// Serial flash window, from `wlink` `flash_op::CH569` (radare2, riscv 32):
/// `lui a5, 0x40001`, then `sb`/`lbu` at `+0x1a` (CTRL) and `+0x18` (DATA),
/// `sw` at `+0x14` (WDATA). That is `0x4000101a`, inside the SYS block. The
/// CH57x/CH58x/CH59x window at `0x40001800` does not decode on this chip: a
/// read there comes back as `ff 00 00 00` and a write does not stick, so an
/// erase aimed at it polls until its counter runs out.
const CTRL: usize = 0x4000_101a;
const DATA: usize = 0x4000_1018;
const WDATA: usize = 0x4000_1014;

/// CH569 SYS is byte-packed from `0x40001000` (CH569DS1 / `ch56x.svd`).
/// CH57x/CH58x/CH59x put the same names 0x40 bytes higher.
const SAM: usize = 0x4000_1000;
const ROMCFG: usize = 0x4000_1004;

/// CodeFlash is `0x00000..0x70000` (448 KiB). DataFlash follows at `0x70000`
/// (32 KiB) and is not mapped here. `0x80000` is the end of the 512 KiB ROM
/// window (CH569DS1 table 2-1); the loader rejects anything at or above it.
const CODE_LIMIT: u32 = 0x0007_0000;
const ROM_LIMIT: u32 = 0x0008_0000;

/// `RB_ROM_CODE_WE | RB_ROM_DATA_WE | RB_CODE_RAM_WE` = bits 3, 2, 1.
/// The official loader ORs `0x8c` and, when locking, keeps bit 7 (`0x80`)
/// while clearing bits 2, 3 and 7's neighbours via `& 0x73`.
const ROM_WE: u8 = 0x8c;
const ROM_LOCK_KEEP: u8 = 0x80;
const ROM_LOCK_MASK: u8 = 0x73;

/// `RB_ROM_CODE_OFS` (CH569DS1, `R8_GLOB_ROM_CFG` bit 4). When set, user code
/// starts at `0x04000` instead of `0`, and the official erase and program
/// helpers return `0xfb` without touching flash. Cleared only by power-on
/// reset, so a running session cannot undo it.
const ROM_CODE_OFS: u8 = 0x10;

/// Erase block of command `0x20`. The loader's sector flag passes length
/// `0x1000` and takes this path. Command `0xD8` exists too, but only as a
/// 64 KiB step on the bulk erase below `0x70000`, not as the page size.
const ERASE_SIZE: u32 = 0x1000;
/// Bytes issued under one program command (`0x02`) before the status poll.
const PAGE_SIZE: u32 = 0x100;
const PROGRAM_TIMEOUT_MS: u32 = 500;
const ERASE_TIMEOUT_MS: u32 = 1000;

struct Algo;

// Same macro as `algos/*`. It emits the CMSIS entry points and the
// `FlashDevice` static that `target-gen elf` reads. `_IS_INIT` lands in
// `.bss` inside `PrgCode`; the load address is fixed at RAMX `0x20020008`,
// so the absolute address the macro generates is the one this chip uses.
// `algos-ch5xx/ch5xx` cannot do this: that binary is position-independent
// and CH571/CH573 SRAM is not at `0x20000000`.
algorithm!(Algo, {
    device_name: "CH565/CH569 CodeFlash",
    device_type: DeviceType::Onchip,
    flash_address: 0,
    flash_size: CODE_LIMIT,
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
        if addr >= CODE_LIMIT {
            return Err(ErrorCode::new(1).unwrap());
        }
        Ok(Self)
    }

    fn erase_all(&mut self) -> Result<(), ErrorCode> {
        // The loader's bulk path erases 64 KiB at a time with command `0xD8`
        // while the start stays below `0x70000`, then finishes the tail with
        // `0x20`. 448 KiB is an exact multiple of 64 KiB, so the tail is empty.
        let mut a = 0u32;
        while a < CODE_LIMIT {
            self.erase_block(0xd8, a)?;
            a += 0x1_0000;
        }
        Ok(())
    }

    fn erase_sector(&mut self, sector_addr: u32) -> Result<(), ErrorCode> {
        let aligned = sector_addr & !(ERASE_SIZE - 1);
        self.erase_block(0x20, aligned)
    }

    fn program_page(&mut self, dest: u32, data: &[u8]) -> Result<(), ErrorCode> {
        let size = data.len() as u32;
        if size == 0 {
            return Ok(());
        }
        if dest >= CODE_LIMIT
            || CODE_LIMIT - dest < size
            || ((dest | size) & 3) != 0
            || unsafe { code_offset_set() }
        {
            return Err(ErrorCode::new(1).unwrap());
        }
        // `0x80000` is the ROM window end used by the official loader. CodeFlash
        // ends earlier; this rejects a wrap that the subtraction above cannot see.
        if dest.saturating_add(size) > ROM_LIMIT {
            return Err(ErrorCode::new(1).unwrap());
        }
        unsafe {
            unlock();
            let mut offset = 0u32;
            let mut ok = true;
            while offset < size {
                let chunk = if size - offset > PAGE_SIZE {
                    PAGE_SIZE
                } else {
                    size - offset
                };
                addr(0x02, dest + offset);
                let mut i = 0u32;
                while i < chunk {
                    let p = data.as_ptr().add((offset + i) as usize);
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
            if ok {
                Ok(())
            } else {
                Err(ErrorCode::new(1).unwrap())
            }
        }
    }

    /// Read flash into `data`. probe-rs calls this for `--verify`, because a
    /// halted debug read of CodeFlash returns the `11 bf f9 f7` placeholder
    /// instead of the stored bytes. The loader's verify helper (`0x3ac`) uses
    /// command `0x0b` for the same read.
    fn read_flash(&mut self, address: u32, data: &mut [u8]) -> Result<(), ErrorCode> {
        let size = data.len() as u32;
        if size == 0 {
            return Ok(());
        }
        if address >= CODE_LIMIT || CODE_LIMIT - address < size || (size & 3) != 0 {
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
                // The loader discards the first two data bytes after the address.
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
    /// One erase command. `cmd` is `0x20` for a 4 KiB sector or `0xD8` for a
    /// 64 KiB block; the loader picks between them in the helper at `0x1b0`.
    fn erase_block(&mut self, cmd: u8, addr_at: u32) -> Result<(), ErrorCode> {
        unsafe {
            if addr_at >= CODE_LIMIT || code_offset_set() {
                return Err(ErrorCode::new(1).unwrap());
            }
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

#[inline(always)]
unsafe fn w32(a: usize, v: u32) {
    unsafe { core::ptr::write_volatile(a as *mut u32, v) }
}

/// Start a command. The loader at `0xea` writes CTRL `0`, then `7`, then the
/// command byte to DATA. CH57x/CH58x/CH59x write `5` here; this chip does not.
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

/// Send a 24-bit address command. Command `0x0b` (read) skips the preamble
/// that every other command sends; the official loader does the same.
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

/// Read the word the controller just shifted in. The data register at
/// `0x40001018` only clocks the transfer; the word itself is latched at
/// `0x40001014`, the same address a program writes to. The official loader's
/// verify reads it with one `lw` after four data-register reads. Reading the
/// data register instead returns a mix of bits from the whole word.
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

/// Poll status command `0x05` until bit 0 clears. Returns `0` on timeout.
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

/// Program one word. After the word is stored at `WDATA`, the loader
/// reads CTRL, ORs in `0x10`, polls until CTRL is no longer negative, and
/// writes that value back. It does this four times per word (`0x31e..0x362`).
/// Not the constant `0x15` pulse used by CH57x/CH58x/CH59x.
#[inline(always)]
unsafe fn write_dword(v: u32) {
    unsafe {
        w32(WDATA, v);
        let pulse = r8(CTRL) | 0x10;
        let mut i = 4;
        loop {
            while (r8(CTRL) as i8) < 0 {}
            w8(CTRL, pulse);
            i -= 1;
            if i == 0 {
                break;
            }
        }
    }
}

/// `R8_GLOB_ROM_CFG` bit 4. The loader refuses erase and program while it is set.
#[inline(always)]
unsafe fn code_offset_set() -> bool {
    unsafe { r8(ROMCFG) & ROM_CODE_OFS != 0 }
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
