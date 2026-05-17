#![no_std]

pub use flash_algo_common::*;

pub const USR_BASE: u32 = region("USR_1").address;
const USR_FAST: (u32, u32) = fast(region("USR_1"));
pub const USR_PAGE_SIZE: u32 = USR_FAST.0;
pub const USR_LOAD: u32 = USR_FAST.1;

pub const SYS_BASE: u32 = region("SYS_1").address;
const SYS_FAST: (u32, u32) = fast(region("SYS_1"));
pub const SYS_PAGE_SIZE: u32 = SYS_FAST.0;
pub const SYS_LOAD: u32 = SYS_FAST.1;

pub const OPT_BASE: u32 = region("OPT").address;
const OPT_STD: (u32, u32) = standard(region("OPT"));
pub const OPT_ERASE_SIZE: u32 = OPT_STD.0;
pub const OPT_WRITE_SIZE: u32 = OPT_STD.1;

pub fn wait_busy() {
    while FLASH.statr().read().bsy() {}
    FLASH.statr().write(|w| w.set_eop(true));
}

pub fn wait_wr_busy() {
    while FLASH.statr().read().wr_bsy() {}
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
    FLASH.obkeyr().write(|w| w.set_optkey(KEY1));
    FLASH.obkeyr().write(|w| w.set_optkey(KEY2));
}

pub fn fast_page_erase(addr: u32, _page_size: u32) {
    wait_busy();
    FLASH.ctlr().modify(|w| w.set_page_er(true));
    FLASH.addr().write(|w| w.set_far(addr));
    FLASH.ctlr().modify(|w| w.set_strt(true));
    wait_busy();
    FLASH.ctlr().modify(|w| w.set_page_er(false));
}

/// V3 writes words straight into the mapped flash address (no buffer-load
/// step) and waits WR_BSY each, then commits with PGSTART.
pub fn fast_page_program(addr: u32, data: &[u8], _page_size: u32, load_size: u32) {
    wait_busy();
    FLASH.ctlr().modify(|w| w.set_page_pg(true));

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

    FLASH.ctlr().modify(|w| w.set_pgstart(true));
    wait_busy();
    FLASH.ctlr().modify(|w| w.set_page_pg(false));
}

pub fn mass_erase() {
    wait_busy();
    FLASH.ctlr().modify(|w| w.set_mer(true));
    FLASH.ctlr().modify(|w| w.set_strt(true));
    wait_busy();
    FLASH.ctlr().modify(|w| w.set_mer(false));
}

pub fn options_erase() {
    wait_busy();
    FLASH.ctlr().modify(|w| w.set_ober(true));
    FLASH.ctlr().modify(|w| w.set_strt(true));
    wait_busy();
    FLASH.ctlr().modify(|w| w.set_ober(false));
}

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
