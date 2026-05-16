#![no_std]
#![no_main]

use flash_algo_v00x::*;
use flash_algorithm::{ErrorCode, FlashAlgorithm, Function, algorithm};

struct Algo;

algorithm!(Algo, {
    device_name: "CH32 V00X USR",
    device_type: DeviceType::Onchip,
    flash_address: USR_BASE,
    flash_size: 0,
    page_size: USR_PAGE_SIZE,
    empty_value: 0xFF,
    program_time_out: PROGRAM_TIMEOUT_MS,
    erase_time_out: ERASE_TIMEOUT_MS,
    sectors: [{
        size: USR_PAGE_SIZE,
        address: USR_BASE,
    }]
});

impl FlashAlgorithm for Algo {
    fn new(_addr: u32, _clock: u32, _function: Function) -> Result<Self, ErrorCode> {
        unlock_main();
        Ok(Self)
    }

    fn erase_all(&mut self) -> Result<(), ErrorCode> {
        mass_erase();
        Ok(())
    }

    fn erase_sector(&mut self, addr: u32) -> Result<(), ErrorCode> {
        fast_page_erase(addr, USR_PAGE_SIZE);
        Ok(())
    }

    fn program_page(&mut self, addr: u32, data: &[u8]) -> Result<(), ErrorCode> {
        fast_page_program(addr, data, USR_PAGE_SIZE, USR_LOAD);
        Ok(())
    }
}

impl Drop for Algo {
    fn drop(&mut self) {
        lock_main();
    }
}
