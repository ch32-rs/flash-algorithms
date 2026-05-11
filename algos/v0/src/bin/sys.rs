#![no_std]
#![no_main]

use flash_algo_v0::*;
use flash_algorithm::{ErrorCode, FlashAlgorithm, Function, algorithm};

struct Algo;

algorithm!(Algo, {
    device_name: "CH32 v0 system flash",
    device_type: DeviceType::Onchip,
    flash_address: SYS_BASE,
    flash_size: SYS_SIZE,
    page_size: PAGE_SIZE,
    empty_value: 0xFF,
    program_time_out: PROGRAM_TIMEOUT_MS,
    erase_time_out: ERASE_TIMEOUT_MS,
    sectors: [{
        size: PAGE_SIZE,
        address: SYS_BASE,
    }]
});

impl FlashAlgorithm for Algo {
    fn new(_addr: u32, _clock: u32, _function: Function) -> Result<Self, ErrorCode> {
        unlock_main();
        enable_boot_mode();
        Ok(Self)
    }

    fn erase_all(&mut self) -> Result<(), ErrorCode> {
        Err(ERR_NOT_SUPPORTED)
    }

    fn erase_sector(&mut self, addr: u32) -> Result<(), ErrorCode> {
        page_erase(addr);
        Ok(())
    }

    fn program_page(&mut self, addr: u32, data: &[u8]) -> Result<(), ErrorCode> {
        page_program(addr, data);
        Ok(())
    }
}

/// Arm BOOT_MODE — required before writing the system flash bank. Gated by
/// its own BOOT_MODEKEYP key sequence; v1/l1 don't expose this bit.
fn enable_boot_mode() {
    FLASH.boot_modekeyp().write(|w| w.set_modekeyr(KEY1));
    FLASH.boot_modekeyp().write(|w| w.set_modekeyr(KEY2));
    FLASH.statr().write(|w| w.set_boot_mode(true));
}
