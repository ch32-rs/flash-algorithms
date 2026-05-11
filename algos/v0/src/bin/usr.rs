#![no_std]
#![no_main]

use flash_algo_v0::*;
use flash_algorithm::{ErrorCode, FlashAlgorithm, Function, algorithm};

struct Algo;

// flash_size = 0: xtask writes the per-chip size into the generated YAML.
algorithm!(Algo, {
    device_name: "CH32 v0 user flash",
    device_type: DeviceType::Onchip,
    flash_address: USER_FLASH_BASE,
    flash_size: 0,
    page_size: PAGE_SIZE,
    empty_value: 0xFF,
    program_time_out: PROGRAM_TIMEOUT_MS,
    erase_time_out: ERASE_TIMEOUT_MS,
    sectors: [{
        size: PAGE_SIZE,
        address: USER_FLASH_BASE,
    }]
});

impl FlashAlgorithm for Algo {
    fn new(_addr: u32, _clock: u32, _function: Function) -> Result<Self, ErrorCode> {
        unlock_main();
        Ok(Self)
    }

    fn erase_all(&mut self) -> Result<(), ErrorCode> {
        Err(ERR_NOT_SUPPORTED)
    }

    fn erase_sector(&mut self, addr: u32) -> Result<(), ErrorCode> {
        page_erase(translate_user(addr));
        Ok(())
    }

    fn program_page(&mut self, addr: u32, data: &[u8]) -> Result<(), ErrorCode> {
        page_program(translate_user(addr), data);
        Ok(())
    }
}
