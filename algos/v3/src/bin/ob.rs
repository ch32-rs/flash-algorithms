#![no_std]
#![no_main]

use flash_algo_v3::*;
use flash_algorithm::{ErrorCode, FlashAlgorithm, Function, algorithm};

struct Algo;

algorithm!(Algo, {
    device_name: "CH32 V3 OB",
    device_type: DeviceType::Onchip,
    flash_address: OPT_BASE,
    flash_size: 0,
    page_size: OPT_ERASE_SIZE,
    empty_value: 0xFF,
    program_time_out: PROGRAM_TIMEOUT_MS,
    erase_time_out: ERASE_TIMEOUT_MS,
    sectors: [{
        size: OPT_ERASE_SIZE,
        address: OPT_BASE,
    }]
});

impl FlashAlgorithm for Algo {
    fn new(_addr: u32, _clock: u32, _function: Function) -> Result<Self, ErrorCode> {
        unlock_options();
        Ok(Self)
    }

    fn erase_all(&mut self) -> Result<(), ErrorCode> {
        options_erase();
        options_program(OPT_BASE, &OB_DEFAULTS)?;
        Ok(())
    }

    fn erase_sector(&mut self, _addr: u32) -> Result<(), ErrorCode> {
        options_erase();
        Ok(())
    }

    fn program_page(&mut self, addr: u32, data: &[u8]) -> Result<(), ErrorCode> {
        options_program(addr, data)?;
        Ok(())
    }
}

impl Drop for Algo {
    fn drop(&mut self) {
        lock_main();
    }
}
