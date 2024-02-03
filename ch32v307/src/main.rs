#![no_std]
#![no_main]

use core::num::NonZeroU32;
use panic_halt as _;

use flash_algorithm::*;

use ch32v3::ch32v30x as pac;

struct Algorithm;

const FLASH_KEY1: u32 = 0x45670123;
const FLASH_KEY2: u32 = 0xCDEF89AB;

const ERASE_TIMEOUT: u32 = 0xF00000;

algorithm!(Algorithm, {
    flash_address: 0x0000000,
    flash_size: 0x40000,
    page_size: 0x100,
    // Note: This is not correct, each erased word looks like: 0xe339e339
    empty_value: 0x39,
    sectors: [{
        size: 0x8000,
        address: 0x0000000,
    }]
});

unsafe fn wait_until_not_write_busy() -> Result<(), ErrorCode> {
    let flash = &*pac::FLASH::ptr();
    for _ in 0..ERASE_TIMEOUT {
        let status = flash.statr.read();
        if status.wr_bsy().bit_is_set() {
            continue;
        }
        if status.wrprterr().bit() {
            return Err(ErrorCode::new(status.bits()).unwrap());
        }
        return Ok(());
    }
    return Err(NonZeroU32::new(2_u32).unwrap())
}

unsafe fn wait_until_not_busy() -> Result<(), ErrorCode> {
    let flash = &*pac::FLASH::ptr();
    for _ in 0..ERASE_TIMEOUT {
        let status = flash.statr.read();
        if status.bsy().bit_is_set() && status.eop().bit_is_clear() {
            continue;
        }
        if status.wrprterr().bit() {
            return Err(ErrorCode::new(status.bits()).unwrap());
        }
        flash.statr.modify(|_, w| w.eop().clear_bit());
        return Ok(());
    }
    return Err(NonZeroU32::new(1_u32).unwrap())
}

impl FlashAlgorithm for Algorithm {
    fn new(_address: u32, _clock: u32, _function: Function) -> Result<Self, ErrorCode> {
        // Unlock the flash
        unsafe {
            let flash = &*pac::FLASH::ptr();
            flash.keyr.write(|w| w.bits(FLASH_KEY1));
            flash.keyr.write(|w| w.bits(FLASH_KEY2));

            flash.modekeyr.write(|w| w.bits(FLASH_KEY1));
            flash.modekeyr.write(|w| w.bits(FLASH_KEY2));
        }

        Ok(Self)
    }

    fn erase_sector(&mut self, addr: u32) -> Result<(), ErrorCode> {
        let addr = addr + 0x8000000;
        if addr & 0x7FFF != 0 {
            return Err(ErrorCode::from(NonZeroU32::new(addr).unwrap()));
        }
        unsafe { wait_until_not_busy()?; }

        unsafe {
            let flash = &*pac::FLASH::ptr();
            flash.ctlr.modify(|_, w| w.ber32().set_bit());
            flash.addr.write(|w| w.bits(addr));
            flash.ctlr.modify(|_, w| w.strt().set_bit());

            wait_until_not_busy()?;

            flash.ctlr.modify(|_, w| w.ber32().clear_bit());
        }
        Ok(())
    }

    fn program_page(&mut self, addr: u32, data: &[u8]) -> Result<(), ErrorCode> {
        let flash = unsafe { &*pac::FLASH::ptr() };
        if flash.ctlr.read().lock().bit_is_set() || flash.ctlr.read().flock().bit_is_set() {
            return Err(ErrorCode::from(NonZeroU32::new(3).unwrap()));
        }
        let addr = (addr + 0x8000000) as usize;
        if addr & 0xFF != 0 {
            return Err(ErrorCode::from(NonZeroU32::new(addr as u32).unwrap()));
        }
        unsafe {
            flash.ctlr.modify(|_, w| w.page_pg().set_bit());
            wait_until_not_busy().map_err(|e| e.checked_add(10).unwrap())?;
            flash
        };

        for (word, addr) in data.chunks_exact(4).zip((addr..).step_by(4)) {
            let word = u32::from_le_bytes(word.try_into().unwrap());
            unsafe {
                (addr as *mut u32).write_volatile(word);
                wait_until_not_write_busy()?;
            };
        }

        unsafe {
            flash.ctlr.modify(|_, w| w.pgstart().set_bit());
            wait_until_not_busy()?;
            flash.ctlr.modify(|_, w| w.page_pg().clear_bit());
        }
        Ok(())
    }
}

impl Drop for Algorithm {
    fn drop(&mut self) {
        unsafe {
            let flash = &*pac::FLASH::ptr();
            // Lock the flash
            flash.ctlr.modify(|_, w| w.lock().set_bit());
        }
    }
}
