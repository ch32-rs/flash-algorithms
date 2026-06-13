#![no_std]

pub use flash_algo_common::*;

pub const USR_BASE: u32 = region("USR_1").address;
const USR_FAST: (u32, u32) = fast(region("USR_1"));
pub const USR_PAGE_SIZE: u32 = USR_FAST.0;
pub const USR_LOAD: u32 = USR_FAST.1;
const USR_STD: (u32, u32) = standard(region("USR_1"));
/// Pair FTPG (256B) with PER (8K) instead of BER (64K) — finer alignment for
/// app/bootloader/config layouts is worth the 8× erase calls.
pub const USR_ERASE_SIZE: u32 = USR_STD.0;

pub const SYS_BASE: u32 = region("SYS_1").address;
const SYS_FAST: (u32, u32) = fast(region("SYS_1"));
pub const SYS_PAGE_SIZE: u32 = SYS_FAST.0;
pub const SYS_LOAD: u32 = SYS_FAST.1;
const SYS_STD: (u32, u32) = standard(region("SYS_1"));
pub const SYS_ERASE_SIZE: u32 = SYS_STD.0;

pub const OPT_BASE: u32 = region("OPT").address;
const OPT_STD: (u32, u32) = standard(region("OPT"));
pub const OPT_ERASE_SIZE: u32 = OPT_STD.0;
pub const OPT_WRITE_SIZE: u32 = OPT_STD.1;

pub fn wait_busy() {
    while FLASH.statr().read().bsy() {}
    FLASH.statr().write(|w| w.set_eop(true));
}

pub fn wait_wr_busy() {
    while FLASH.statr().read().wrbsy() {}
}

pub fn unlock_main() {
    FLASH.keyr().write(|w| w.set_keyr(KEY1));
    FLASH.keyr().write(|w| w.set_keyr(KEY2));
    FLASH.modekeyr().write(|w| w.set_modekeyr(KEY1));
    FLASH.modekeyr().write(|w| w.set_modekeyr(KEY2));
}

pub fn lock_main() {
    FLASH.ctlr().modify(|w| {
        w.set_lock(true);
        w.set_flock(true);
    });
}

pub fn unlock_options() {
    FLASH.keyr().write(|w| w.set_keyr(KEY1));
    FLASH.keyr().write(|w| w.set_keyr(KEY2));
    FLASH.obkeyr().write(|w| w.set_obtkey(KEY1));
    FLASH.obkeyr().write(|w| w.set_obtkey(KEY2));
}

pub fn page_erase(addr: u32) {
    wait_busy();
    FLASH.ctlr().modify(|w| w.set_per(true));
    FLASH.addr().write(|w| w.set_far(addr));
    FLASH.ctlr().modify(|w| w.set_strt(true));
    wait_busy();
    FLASH.ctlr().modify(|w| w.set_per(false));
}

/// RM 46.5.6: stream 32-bit words straight to the flash address with WRBSY
/// gates, then commit with PGSTRT.
pub fn fast_page_program(addr: u32, data: &[u8], _page_size: u32, load_size: u32) {
    wait_busy();
    FLASH.ctlr().modify(|w| w.set_ftpg(true));

    let mut cur = addr;
    let mut src = data.as_ptr() as *const u32;
    let words = data.len() as u32 / load_size;
    for _ in 0..words {
        let v = unsafe { src.read() };
        unsafe { core::ptr::write_volatile(cur as *mut u32, v) };
        wait_wr_busy();
        cur += load_size;
        src = unsafe { src.add(1) };
    }

    FLASH.ctlr().modify(|w| w.set_pgstrt(true));
    wait_busy();
    FLASH.ctlr().modify(|w| w.set_ftpg(false));
}

pub fn options_erase() {
    wait_busy();
    FLASH.ctlr().modify(|w| w.set_ober(true));
    FLASH.ctlr().modify(|w| w.set_strt(true));
    wait_busy();
    FLASH.ctlr().modify(|w| w.set_ober(false));
}

/// RM 46.6.2: FPEC takes the low byte, computes the complement byte itself,
/// so the algo only needs to write each value once per OB slot.
fn options_program_halfword(addr: u32, val: u16) {
    wait_busy();
    FLASH.ctlr().modify(|w| w.set_obpg(true));
    unsafe { core::ptr::write_volatile(addr as *mut u16, val) };
    wait_busy();
    FLASH.ctlr().modify(|w| w.set_obpg(false));
}

pub fn options_program(addr: u32, data: &[u8]) -> Result<(), core::num::NonZeroU32> {
    let step = OPT_WRITE_SIZE as usize;
    if (addr as usize % step) != 0 || data.len() % step != 0 {
        return Err(ERR_NOT_SUPPORTED);
    }
    let mut cur = addr;
    let mut src = data.as_ptr() as *const u16;
    let halfwords = data.len() / step;
    for _ in 0..halfwords {
        let hw = unsafe { src.read() };
        options_program_halfword(cur, hw);
        cur += step as u32;
        src = unsafe { src.add(1) };
    }
    Ok(())
}
