#![no_std]

pub use flash_algo_common::*;

pub const USR_BASE: u32 = region("USR_1").address;
const USR_FAST: (u32, u32) = fast(region("USR_1"));
pub const USR_PAGE_SIZE: u32 = USR_FAST.0;
pub const USR_LOAD: u32 = USR_FAST.1;

/// V1 has two non-contiguous SYS regions (SYS_1 + SYS_2) sharing one FPEC
/// programming flow; both share SYS_1's geometry.
pub const SYS_BASE: u32 = region("SYS_1").address;
const SYS_FAST: (u32, u32) = fast(region("SYS_1"));
pub const SYS_PAGE_SIZE: u32 = SYS_FAST.0;
pub const SYS_LOAD: u32 = SYS_FAST.1;

pub const OPT_BASE: u32 = region("OPT").address;
const OPT_STD: (u32, u32) = standard(region("OPT"));
pub const OPT_ERASE_SIZE: u32 = OPT_STD.0;
pub const OPT_WRITE_SIZE: u32 = OPT_STD.1;

/// Undocumented read-cache register at FLASH+0x34; used by WCH's HAL
/// (openwch/ch32v103 `ch32v10x_flash.c`). Without poking this post-erase or
/// post-program, the prefetch can serve stale data.
const FLASH_RDCACHE_REG: *mut u32 = 0x4002_2034 as *mut u32;
const RDCACHE_XOR_MASK: u32 = 0x0000_1000;

pub fn wait_busy() {
    while FLASH.statr().read().bsy() {}
    FLASH.statr().write(|w| w.set_eop(true));
}

pub fn unlock_main() {
    FLASH.keyr().write(|w| w.set_keyr(KEY1));
    FLASH.keyr().write(|w| w.set_keyr(KEY2));
    FLASH.modekeyr().write(|w| w.set_modekeyr(KEY1));
    FLASH.modekeyr().write(|w| w.set_modekeyr(KEY2));
}

pub fn unlock_options() {
    FLASH.keyr().write(|w| w.set_keyr(KEY1));
    FLASH.keyr().write(|w| w.set_keyr(KEY2));
    FLASH.obkeyr().write(|w| w.set_obkeyr(KEY1));
    FLASH.obkeyr().write(|w| w.set_obkeyr(KEY2));
}

pub fn lock_main() {
    FLASH.ctlr().modify(|w| {
        w.set_lock(true);
        w.set_flock(true);
    });
}

pub fn invalidate_read_cache(addr: u32) {
    let src = addr ^ RDCACHE_XOR_MASK;
    let val = unsafe { core::ptr::read_volatile(src as *const u32) };
    unsafe { core::ptr::write_volatile(FLASH_RDCACHE_REG, val) };
}

pub fn fast_page_erase(addr: u32, _page_size: u32) {
    wait_busy();
    FLASH.ctlr().write(|w| w.set_fter(true));
    FLASH.addr().write(|w| w.set_far(addr));
    FLASH.ctlr().write(|w| {
        w.set_fter(true);
        w.set_strt(true);
    });
    wait_busy();
    FLASH.ctlr().modify(|w| w.set_fter(false));
    invalidate_read_cache(addr);
}

/// V1 streams 4 words into the FPEC buffer per BUFLOAD strobe (unlike
/// v0/v00x which BUFLOAD every word).
pub fn fast_page_program(addr: u32, data: &[u8], page_size: u32, load_size: u32) {
    let page_base = addr & !(page_size - 1);

    wait_busy();
    FLASH.ctlr().write(|w| w.set_ftpg(true));
    FLASH.ctlr().write(|w| {
        w.set_ftpg(true);
        w.set_bufrst(true);
    });
    wait_busy();

    let mut buf_addr = addr;
    let mut src = data.as_ptr() as *const u32;
    let mut loaded: u32 = 0;
    let words = data.len() as u32 / 4;
    for _ in 0..words {
        let v = unsafe { src.read() };
        unsafe { core::ptr::write_volatile(buf_addr as *mut u32, v) };
        buf_addr += 4;
        src = unsafe { src.add(1) };
        loaded += 4;
        if loaded % load_size == 0 {
            FLASH.ctlr().write(|w| {
                w.set_ftpg(true);
                w.set_bufload(true);
            });
            wait_busy();
        }
    }

    FLASH.addr().write(|w| w.set_far(page_base));
    FLASH.ctlr().write(|w| {
        w.set_ftpg(true);
        w.set_strt(true);
    });
    wait_busy();
    FLASH.ctlr().modify(|w| w.set_ftpg(false));
    invalidate_read_cache(page_base);
}

pub fn mass_erase() {
    wait_busy();
    FLASH.ctlr().modify(|w| w.set_mer(true));
    FLASH.ctlr().modify(|w| w.set_strt(true));
    wait_busy();
    FLASH.ctlr().modify(|w| w.set_mer(false));
    invalidate_read_cache(USR_BASE);
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
