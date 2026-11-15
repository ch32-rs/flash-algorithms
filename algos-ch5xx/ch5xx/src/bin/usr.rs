#![no_std]
#![no_main]

use core::panic::PanicInfo;

#[panic_handler]
fn ph(_: &PanicInfo) -> ! {
    loop {}
}

const CTRL: usize = 0x4000_1806;
const DATA: usize = 0x4000_1804;
const WDATA: usize = 0x4000_1800;
const ROMCFG: usize = 0x4000_1044;
const GLOB_CFG_INFO: usize = 0x4000_1045;
const CHIP_ID: usize = 0x4000_1041;
const SAM: usize = 0x4000_1040;
/// `ROM_CFG_CHIP_ID`; the low nibble is `0x08` only on CH584X.
const ROM_CFG_CHIP_ID: usize = 0x0007_F024;

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

/// Set once in `init`.
static mut IS_584X: bool = false;

#[inline(always)]
unsafe fn is_584x() -> bool {
    unsafe { IS_584X }
}

/// CH584X compatibility: check the basic chip ID first, so only the `0x92`
/// group reads the ISP585 model code at `0x7F024`.
///
/// `0x7F024` is a Flash read, so this runs once from `init`, before the engine
/// is unlocked. Reading Flash between `unlock_and_reset` and the erase command
/// leaves the CH592 engine busy forever. WCH's `libISP585` reads it before its
/// own unlock for the same reason.
#[inline(always)]
unsafe fn detect_584x() -> bool {
    unsafe {
        if r8(CHIP_ID) != 0x92 {
            return false;
        }
        // Flash read: must precede `unlock_and_reset`, or the CH592 engine hangs.
        core::ptr::read_volatile(ROM_CFG_CHIP_ID as *const u32) & 0x0f == 0x08
    }
}

/// Returns the CodeFlash size limit in bytes based on `R8_CHIP_ID` (`0x40001041`).
///
/// Note on CH571/CH573 (`0x71`/`0x73`): the SVD splits `RESET_STATUS`/`GLOB_ROM_CFG`
/// at `SYS + 4`, but both `CH573SFR.h` and `wlink`'s `flash_op::CH573` (`ISP573.o`)
/// access `R8_SAFE_ACCESS_SIG` at `0x40001040`, `R8_CHIP_ID` at `0x40001041`,
/// `R8_GLOB_ROM_CFG` at `0x40001044`, and `R32_FLASH_DATA`/`R8_FLASH_CTRL` at
/// `0x40001800..0x40001806` — identical to CH570/572, CH581/582/583/584/585, and CH591/592.
///
/// Note: CH595 uses a newer hardware pre-program/page-latch Flash controller at
/// `0x40001800..0x40001820` (`CH59x_flash.c`) and is not covered by this crate.
#[inline(always)]
unsafe fn flash_limit() -> u32 {
    if unsafe { is_584x() } {
        return unsafe {
            // 480 KiB, or ISP585's conservative 448 KiB while
            // `RB_BOOT_LOADER` (`R8_GLOB_CFG_INFO` bit 5) is set.
            if r8(GLOB_CFG_INFO) & 0x20 != 0 {
                0x0007_0000
            } else {
                0x0007_8000
            }
        };
    }
    let id = unsafe { r8(CHIP_ID) };
    if id == 0x71 || id == 0x91 {
        0x0003_0000 // CH571, CH591: 192 KiB
    } else if id == 0x70 || id == 0x72 {
        0x0003_c000 // CH570, CH572: 240 KiB
    } else {
        0x0007_0000 // CH573, CH581, CH582, CH583, CH584, CH585, CH592: 448 KiB
    }
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
        // CH584X addresses CodeFlash at engine `CPU + 0x80000`.
        if is_584x() {
            a += 0x0008_0000;
        }
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

/// `0x0B` frame read: five address bytes (24-bit address plus two zero bytes).
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

#[unsafe(no_mangle)]
pub extern "C" fn init(addr: u32, _clk: u32, _fn_code: u32) -> u32 {
    unsafe {
        IS_584X = detect_584x();
        if addr >= flash_limit() { 1 } else { 0 }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn uninit(_fn_code: u32) -> u32 {
    unsafe {
        w8(CTRL, 0);
        0
    }
}

/// One erase command. `cmd` is `0x20` for a 4 KiB sector or `0xD8` for a
/// 64 KiB block.
#[inline(always)]
unsafe fn erase_block(cmd: u8, at: u32) -> u32 {
    unsafe {
        unlock_and_reset();
        addr(cmd, at);
        let st = wait();
        lock();
        if st == 0 { 1 } else { 0 }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn erase_sector(sector_addr: u32) -> u32 {
    let aligned = sector_addr & 0x0fff_f000;
    unsafe {
        if aligned >= flash_limit() {
            return 1;
        }
        erase_block(0x20, aligned)
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn erase_all() -> u32 {
    let limit = unsafe { flash_limit() };
    let mut a = 0u32;
    while a < limit {
        // Erase 64 KiB blocks while a whole block is left, then the remainder
        // sector by sector. 448 KiB and 192 KiB are exact multiples of 64 KiB;
        // 240 KiB leaves a 12 KiB tail.
        let (cmd, step) = if a & 0xffff == 0 && limit - a >= 0x1_0000 {
            (0xd8, 0x1_0000)
        } else {
            (0x20, 0x1000)
        };
        let r = unsafe { erase_block(cmd, a) };
        if r != 0 {
            return r;
        }
        a += step;
    }
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn program_page(dest: u32, size: u32, buf: *const u8) -> u32 {
    if size == 0 {
        return 0;
    }
    let limit = unsafe { flash_limit() };
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
            addr(0x02, dest + offset);
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

/// Read CodeFlash through the engine. A halted debug read of the CH5xx
/// CodeFlash area returns a scrambled placeholder, so this is the `--verify`
/// path for the CH584X entry.
#[unsafe(no_mangle)]
pub extern "C" fn read_flash(start: u32, size: u32, buf: *mut u8) -> u32 {
    if size == 0 {
        return 0;
    }
    let limit = unsafe { flash_limit() };
    if start >= limit || limit - start < size {
        return 1;
    }
    if !unsafe { is_584x() } {
        for i in 0..size {
            unsafe { *buf.add(i as usize) = r8((start + i) as usize) };
        }
        return 0;
    }
    unsafe {
        unlock_and_reset();
        read_addr(0x0b, start + 0x0008_0000);
        let mut i = 0u32;
        while i < size {
            *buf.add(i as usize) = r#in();
            i += 1;
        }
        end();
        lock();
    }
    0
}
