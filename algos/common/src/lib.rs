#![no_std]

pub use ch32_metapac::FLASH;

pub const KEY1: u32 = 0x4567_0123;
pub const KEY2: u32 = 0xCDEF_89AB;

pub const PROGRAM_TIMEOUT_MS: u32 = 1000;
pub const ERASE_TIMEOUT_MS: u32 = 2000;

/// Writable flash alias. metapac METADATA reports the boot-aliased `0x0`;
/// writes must target this address.
pub const USER_FLASH_BASE: u32 = 0x0800_0000;

pub const ERR_NOT_SUPPORTED: core::num::NonZeroU32 = core::num::NonZeroU32::MIN;

/// Shift boot-aliased addresses (< 0x0800_0000) to the writable alias;
/// 0x1FFF_xxxx (system / OB) passes through.
pub fn translate_user(addr: u32) -> u32 {
    if addr < USER_FLASH_BASE { addr + USER_FLASH_BASE } else { addr }
}

pub fn unlock_main() {
    FLASH.keyr().write(|w| w.set_keyr(KEY1));
    FLASH.keyr().write(|w| w.set_keyr(KEY2));
    FLASH.modekeyr().write(|w| w.set_modekeyr(KEY1));
    FLASH.modekeyr().write(|w| w.set_modekeyr(KEY2));
}

pub fn lock() {
    FLASH.ctlr().write(|w| {
        w.set_lock(true);
        w.set_flock(true);
    });
}
