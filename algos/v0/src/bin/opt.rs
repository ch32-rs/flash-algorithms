#![no_std]
#![no_main]

use flash_algo_v0::*;
use flash_algorithm::{ErrorCode, FlashAlgorithm, Function, algorithm};

struct Algo;

algorithm!(Algo, {
    device_name: "CH32 v0 option bytes",
    device_type: DeviceType::Onchip,
    flash_address: OPT_BASE,
    flash_size: OPT_SIZE,
    page_size: OPT_SIZE,
    empty_value: 0xFF,
    program_time_out: PROGRAM_TIMEOUT_MS,
    erase_time_out: ERASE_TIMEOUT_MS,
    sectors: [{
        size: OPT_SIZE,
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
        Ok(())
    }

    fn erase_sector(&mut self, _addr: u32) -> Result<(), ErrorCode> {
        // OB is one hardware sector — addr ignored, whole region erases.
        options_erase();
        Ok(())
    }

    fn program_page(&mut self, addr: u32, data: &[u8]) -> Result<(), ErrorCode> {
        if (addr & 1) != 0 || (data.len() & 1) != 0 {
            return Err(ERR_NOT_SUPPORTED);
        }
        let mut cur = addr;
        let mut i = 0;
        while i < data.len() {
            let hw = u16::from_le_bytes([data[i], data[i + 1]]);
            options_program_halfword(cur, hw);
            cur += 2;
            i += 2;
        }
        Ok(())
    }
}

fn unlock_options() {
    FLASH.keyr().write(|w| w.set_keyr(KEY1));
    FLASH.keyr().write(|w| w.set_keyr(KEY2));
    FLASH.obkeyr().write(|w| w.set_optkey(KEY1));
    FLASH.obkeyr().write(|w| w.set_optkey(KEY2));
}

fn options_erase() {
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
