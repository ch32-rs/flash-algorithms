#![no_std]
#![no_main]

use flash_algo_v0::*;
use flash_algorithm::{ErrorCode, FlashAlgorithm, Function, algorithm};

/// `saved_boot` restores `STATR.BOOT_MODE` in `Drop` — without it, flashing
/// SYS would silently redirect the next reset into the bootloader.
struct Algo {
    saved_boot: bool,
}

algorithm!(Algo, {
    device_name: "CH32 V0 SYS",
    device_type: DeviceType::Onchip,
    flash_address: SYS_BASE,
    flash_size: 0,
    page_size: SYS_PAGE_SIZE,
    empty_value: 0xFF,
    program_time_out: PROGRAM_TIMEOUT_MS,
    erase_time_out: ERASE_TIMEOUT_MS,
    sectors: [{
        size: SYS_PAGE_SIZE,
        address: SYS_BASE,
    }]
});

impl FlashAlgorithm for Algo {
    fn new(_addr: u32, _clock: u32, _function: Function) -> Result<Self, ErrorCode> {
        let saved_boot = boot_mode();
        unlock_main();
        set_boot_mode(true);
        Ok(Self { saved_boot })
    }

    fn erase_all(&mut self) -> Result<(), ErrorCode> {
        Err(ERR_NOT_SUPPORTED)
    }

    fn erase_sector(&mut self, addr: u32) -> Result<(), ErrorCode> {
        fast_page_erase(addr, SYS_PAGE_SIZE);
        Ok(())
    }

    fn program_page(&mut self, addr: u32, data: &[u8]) -> Result<(), ErrorCode> {
        fast_page_program(addr, data, SYS_PAGE_SIZE, SYS_LOAD);
        Ok(())
    }
}

impl Drop for Algo {
    fn drop(&mut self) {
        set_boot_mode(self.saved_boot);
        lock_main();
    }
}
