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
const OPT_FAST: (u32, u32) = fast(region("OPT"));
pub const OPT_PAGE_SIZE: u32 = OPT_FAST.0;
pub const OPT_LOAD: u32 = OPT_FAST.1;

/// Without restoring defaults, post-erase OB leaves `RDPR != 0xA5` and
/// read-protects the chip on next reset.
pub const OB_DEFAULTS_LEN: usize = ob_default_writes_size();
pub const OB_DEFAULTS: [u8; OB_DEFAULTS_LEN] = ob_default_writes();

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

/// X0 OB also flows through the fast-mode buffer, so both LOCK and FLOCK
/// must be cleared in addition to OBWRE.
pub fn unlock_options() {
    FLASH.keyr().write(|w| w.set_keyr(KEY1));
    FLASH.keyr().write(|w| w.set_keyr(KEY2));
    FLASH.modekeyr().write(|w| w.set_modekeyr(KEY1));
    FLASH.modekeyr().write(|w| w.set_modekeyr(KEY2));
    FLASH.obkeyr().write(|w| w.set_optkey(KEY1));
    FLASH.obkeyr().write(|w| w.set_optkey(KEY2));
}

/// `STATR.BOOT_MODE` survives soft reboots and decides the next-reset boot
/// source — snapshot in `Init`, restore in `UnInit`.
pub fn boot_mode() -> bool {
    FLASH.statr().read().boot_mode()
}

pub fn set_boot_mode(mode: bool) {
    FLASH.boot_modekeyp().write(|w| w.set_modekeyr(KEY1));
    FLASH.boot_modekeyp().write(|w| w.set_modekeyr(KEY2));
    FLASH.statr().write(|w| w.set_boot_mode(mode));
}

pub fn lock_main() {
    FLASH.ctlr().write(|w| {
        w.set_lock(true);
        w.set_flock(true);
    });
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
}

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
    let words = data.len() as u32 / load_size;
    for _ in 0..words {
        let v = unsafe { src.read() };
        unsafe { core::ptr::write_volatile(buf_addr as *mut u32, v) };
        FLASH.ctlr().write(|w| {
            w.set_ftpg(true);
            w.set_bufload(true);
        });
        wait_busy();
        buf_addr += load_size;
        src = unsafe { src.add(1) };
    }

    FLASH.addr().write(|w| w.set_far(page_base));
    FLASH.ctlr().write(|w| {
        w.set_ftpg(true);
        w.set_strt(true);
    });
    wait_busy();
    FLASH.ctlr().modify(|w| w.set_ftpg(false));
}

/// MER targets only USR. Caller must ensure BOOT_MODE=0.
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

pub fn options_program(addr: u32, data: &[u8]) -> Result<(), core::num::NonZeroU32> {
    let step = OPT_LOAD as usize;
    if (addr as usize % step) != 0 || data.len() % step != 0 {
        return Err(ERR_NOT_SUPPORTED);
    }
    fast_page_program(addr, data, OPT_PAGE_SIZE, OPT_LOAD);
    Ok(())
}
