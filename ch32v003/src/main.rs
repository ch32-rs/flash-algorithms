#![no_std]
#![no_main]

use panic_halt as _;

use flash_algorithm::*;
// use rtt_target::{rprintln, rtt_init_print};
use ch32v0::ch32v003 as pac;

const KEY1: u32 = 0x45670123;
const KEY2: u32 = 0xCDEF89AB;

struct Algorithm;

algorithm!(Algorithm, {
    flash_address: 0x00000000,
    flash_size: 0x4000,
    page_size: 64,
    empty_value: 0xFF,
    sectors: [{
        size: 1024,
        address: 0x00000000,
    }]
});

/// Refer: CH32V003RM.PDF
/// Refer: CH32V003EVT.zip:EXAM/FLASH/FLASH_Program
impl FlashAlgorithm for Algorithm {
    fn new(_address: u32, _clock: u32, _function: Function) -> Result<Self, ErrorCode> {
        //rtt_init_print!();
        //rprintln!("Init");
        // TODO: Add setup code for the flash algorithm.
        unsafe {
            // unlock normal program mode
            let rb = &*pac::FLASH::PTR;
            rb.keyr.write(|w| w.bits(KEY1));
            rb.keyr.write(|w| w.bits(KEY2));
            // unlock fast program mode
            rb.modekeyr.write(|w| w.bits(KEY1));
            rb.modekeyr.write(|w| w.bits(KEY2));
        }

        Ok(Self)
    }

    fn erase_all(&mut self) -> Result<(), ErrorCode> {
        unsafe {
            let rb = &*pac::FLASH::PTR;

            if rb.ctlr.read().lock().bit_is_set() {
                return Err(ErrorCode::new(0x1001).unwrap()); // flash is locked
            }

            rb.ctlr.modify(|_, w| w.mer().set_bit()); // master erase
            rb.ctlr.modify(|_, w| w.strt().set_bit()); // start erase
            while rb.statr.read().bsy().bit_is_set() {} // wait for erase done

            rb.statr.modify(|_, w| w.eop().clear_bit()); // clear eop flag
            rb.ctlr.modify(|_, w| w.mer().clear_bit()); // clear erase all mode
        }

        Ok(())
    }

    fn erase_sector(&mut self, mut addr: u32) -> Result<(), ErrorCode> {
        // Address fix, 0x08000000 is remapped to 0x00000000 when running
        if addr < 0x08000000 {
            addr += 0x08000000;
        }
        if addr & 0x3FF != 0 {
            return Err(ErrorCode::new(0x1000).unwrap()); // invalid address
        }
        unsafe {
            let rb = &*pac::FLASH::PTR;

            if rb.ctlr.read().lock().bit_is_set() {
                return Err(ErrorCode::new(0x1001).unwrap()); // flash is locked
            }

            rb.ctlr.modify(|_, w| w.per().set_bit()); // sector erase(1K)
            rb.addr.write(|w| w.bits(addr));
            rb.ctlr.modify(|_, w| w.strt().set_bit());
            while rb.statr.read().bsy().bit_is_set() {}

            rb.statr.modify(|_, w| w.eop().clear_bit());
            rb.ctlr.modify(|_, w| w.per().clear_bit());
        }
        Ok(())
    }

    fn program_page(&mut self, mut addr: u32, data: &[u8]) -> Result<(), ErrorCode> {
        // Address fix, 0x08000000 is remapped to 0x00000000 when running
        if addr < 0x08000000 {
            addr += 0x08000000;
        }

        if addr & 0x3F != 0 {
            return Err(ErrorCode::new(0x1000).unwrap()); // invalid address
        }
        unsafe {
            let rb = &*pac::FLASH::PTR;

            if rb.ctlr.read().lock().bit_is_set() {
                return Err(ErrorCode::new(0x1001).unwrap()); // flash is locked
            }

            rb.ctlr.modify(|_, w| w.page_pg().set_bit());
            rb.ctlr.modify(|_, w| w.bufrst().set_bit());
            while rb.statr.read().bsy().bit_is_set() {} // wait for buffer reset done

            let mut start_addr = addr as *mut u32;
            let data_word = data.as_ptr() as *const u32;
            for i in 0..16 {
                start_addr.write_volatile(data_word.offset(i).read_volatile());
                start_addr = start_addr.offset(1); // inc

                rb.ctlr.modify(|_, w| w.bufload().set_bit());
                while rb.statr.read().bsy().bit_is_set() {} // wait for buffer reset done
            }

            rb.ctlr.modify(|_, w| w.page_pg().set_bit());
            rb.addr.write(|w| w.bits(addr));
            rb.ctlr.modify(|_, w| w.strt().set_bit());
            while rb.statr.read().bsy().bit_is_set() {} // wait for program done

            rb.ctlr.modify(|_, w| w.page_pg().clear_bit());

            // verify
            let mut start_addr = addr as *mut u32;
            for i in 0..16 {
                let data = data.as_ptr().offset(i * 4) as *const u32;
                if start_addr.read_volatile() != data.read_volatile() {
                    loop {} // verify failed
                }
                start_addr = start_addr.offset(1);
            }
        }
        Ok(())
    }
}

impl Drop for Algorithm {
    fn drop(&mut self) {
        unsafe {
            let rb = &*pac::FLASH::PTR;

            rb.ctlr.modify(|_, w| w.lock().set_bit().flock().set_bit());
        }
    }
}
