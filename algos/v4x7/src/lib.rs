#![no_std]

pub use flash_algo_common::*;

pub const USR_BASE: u32 = region("USR_1").address;
pub const USR_PAGE_SIZE: u32 = fast(region("USR_1")).0;
pub const USR_ERASE_SIZE: u32 = standard(region("USR_1")).0;
pub const SYS_BASE: u32 = region("SYS_1").address;
pub const SYS_PAGE_SIZE: u32 = fast(region("SYS_1")).0;
pub const SYS_ERASE_SIZE: u32 = standard(region("SYS_1")).0;
pub const OPT_BASE: u32 = region("OPT").address;
pub const OPT_ERASE_SIZE: u32 = standard(region("OPT")).0;
pub const OPT_WRITE_SIZE: u32 = standard(region("OPT")).1;

const ERR_WRITE_PROTECTED: core::num::NonZeroU32 = core::num::NonZeroU32::new(2).unwrap();

const _: () = {
    assert!(USR_PAGE_SIZE == 256 && SYS_PAGE_SIZE == 256);
    assert!(USR_ERASE_SIZE == 4096 && SYS_ERASE_SIZE == 4096);
    assert!(OPT_WRITE_SIZE == 2);
};

fn wait_busy() {
    while FLASH.statr().read().bsy() {}
}

fn clear_status() {
    FLASH.statr().write(|w| {
        w.set_eop(true);
        w.set_wrprterr(true);
    });
}

fn check_status() -> Result<(), core::num::NonZeroU32> {
    wait_busy();
    let protected = FLASH.statr().read().wrprterr();
    clear_status();
    if protected {
        Err(ERR_WRITE_PROTECTED)
    } else {
        Ok(())
    }
}

pub fn unlock_main() {
    FLASH.keyr().write(|w| w.set_keyr(KEY1));
    FLASH.keyr().write(|w| w.set_keyr(KEY2));
    FLASH.modekeyr().write(|w| w.set_modekeyr(KEY1));
    FLASH.modekeyr().write(|w| w.set_modekeyr(KEY2));
    wait_busy();
    clear_status();
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
    wait_busy();
    clear_status();
}

/// V4x7 has PER (4 KiB) and BER32, but no V3 fast-page erase bit.
pub fn page_erase(addr: u32) -> Result<(), core::num::NonZeroU32> {
    if !addr.is_multiple_of(USR_ERASE_SIZE) {
        return Err(ERR_NOT_SUPPORTED);
    }
    wait_busy();
    clear_status();
    FLASH.ctlr().modify(|w| w.set_per(true));
    FLASH.addr().write(|w| w.set_far(addr));
    FLASH.ctlr().modify(|w| w.set_strt(true));
    let result = check_status();
    FLASH.ctlr().modify(|w| w.set_per(false));
    result
}

/// Follow WCH's FLASH_ProgramPage_Fast: 64 words, WR_BSY gates, then PGSTRT.
/// No V3 PAGE_ER, BTER, or BTPG bits exist on this controller.
pub fn fast_page_program(addr: u32, data: &[u8]) -> Result<(), core::num::NonZeroU32> {
    if !addr.is_multiple_of(USR_PAGE_SIZE) || data.len() != USR_PAGE_SIZE as usize {
        return Err(ERR_NOT_SUPPORTED);
    }
    wait_busy();
    clear_status();
    FLASH.ctlr().modify(|w| w.set_ftpg(true));
    wait_busy();
    while FLASH.statr().read().wr_bsy() {}
    for (i, bytes) in data.as_chunks::<4>().0.iter().enumerate() {
        let word = u32::from_le_bytes(*bytes);
        unsafe { core::ptr::write_volatile((addr as usize + i * 4) as *mut u32, word) };
        while FLASH.statr().read().wr_bsy() {}
    }
    FLASH.ctlr().modify(|w| w.set_pgstrt(true));
    let result = check_status();
    FLASH.ctlr().modify(|w| w.set_ftpg(false));
    result
}

/// Erase USR only, never SYS or OPT. The zero-wait split does not change
/// the total 992 KiB USR range; use PER rather than an unvalidated MER path.
pub fn erase_user_flash() -> Result<(), core::num::NonZeroU32> {
    let size = region("USR_1").size + region("USR_2").size;
    for offset in (0..size).step_by(USR_ERASE_SIZE as usize) {
        page_erase(USR_BASE + offset)?;
    }
    Ok(())
}

pub fn options_erase() -> Result<(), core::num::NonZeroU32> {
    wait_busy();
    clear_status();
    FLASH.ctlr().modify(|w| w.set_ober(true));
    FLASH.ctlr().modify(|w| w.set_strt(true));
    let result = check_status();
    FLASH.ctlr().modify(|w| w.set_ober(false));
    result
}

pub fn options_program(addr: u32, data: &[u8]) -> Result<(), core::num::NonZeroU32> {
    if !addr.is_multiple_of(OPT_WRITE_SIZE) || !data.len().is_multiple_of(OPT_WRITE_SIZE as usize) {
        return Err(ERR_NOT_SUPPORTED);
    }
    for (i, bytes) in data.as_chunks::<2>().0.iter().enumerate() {
        wait_busy();
        clear_status();
        FLASH.ctlr().modify(|w| w.set_obpg(true));
        let halfword = u16::from_le_bytes(*bytes);
        unsafe { core::ptr::write_volatile((addr as usize + i * 2) as *mut u16, halfword) };
        let result = check_status();
        FLASH.ctlr().modify(|w| w.set_obpg(false));
        result?;
    }
    Ok(())
}
