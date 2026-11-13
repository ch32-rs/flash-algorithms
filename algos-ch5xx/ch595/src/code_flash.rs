//! CodeFlash support for CH595/CH596.
//!
//! Both parts expose 240 KiB of user CodeFlash at `0..0x3C000`, erased in
//! 2 KiB sectors and programmed through a 256-byte page latch. There is no
//! user DataFlash; the 16 KiB BootLoader at `0x3C000..0x40000` is not mapped.

use flash_algorithm::{ErrorCode, FlashAlgorithm, Function, algorithm};

use crate::flash_rom;

const CODE_LIMIT: u32 = 0x0003_c000;
const ERASE_SIZE: u32 = flash_rom::SECTOR_SIZE;
const PAGE_SIZE: u32 = flash_rom::PAGE_SIZE;
const PROGRAM_TIMEOUT_MS: u32 = 500;
const ERASE_TIMEOUT_MS: u32 = 1000;

const ID_CH595: u8 = 0x97;
const ID_CH596: u8 = 0x96;

struct Algo;

algorithm!(Algo, {
    device_name: "CH595/CH596 CodeFlash",
    device_type: DeviceType::Onchip,
    flash_address: 0,
    flash_size: CODE_LIMIT,
    page_size: PAGE_SIZE,
    empty_value: 0xFF,
    program_time_out: PROGRAM_TIMEOUT_MS,
    erase_time_out: ERASE_TIMEOUT_MS,
    sectors: [{
        size: ERASE_SIZE,
        address: 0,
    }]
});

impl FlashAlgorithm for Algo {
    fn new(addr: u32, _clock: u32, _function: Function) -> Result<Self, ErrorCode> {
        let id = unsafe { flash_rom::chip_id() };
        if id != ID_CH595 && id != ID_CH596 {
            return Err(ErrorCode::new(1).unwrap());
        }
        if addr >= CODE_LIMIT {
            return Err(ErrorCode::new(1).unwrap());
        }
        Ok(Self)
    }

    fn erase_all(&mut self) -> Result<(), ErrorCode> {
        if unsafe { flash_rom::erase_range(0, CODE_LIMIT) } {
            Ok(())
        } else {
            Err(ErrorCode::new(1).unwrap())
        }
    }

    fn erase_sector(&mut self, sector_addr: u32) -> Result<(), ErrorCode> {
        let aligned = sector_addr & !(ERASE_SIZE - 1);
        if aligned >= CODE_LIMIT || CODE_LIMIT - aligned < ERASE_SIZE {
            return Err(ErrorCode::new(1).unwrap());
        }
        if unsafe { flash_rom::erase_sector(aligned) } {
            Ok(())
        } else {
            Err(ErrorCode::new(1).unwrap())
        }
    }

    fn program_page(&mut self, dest: u32, data: &[u8]) -> Result<(), ErrorCode> {
        let size = data.len() as u32;
        if size == 0 {
            return Ok(());
        }
        if dest >= CODE_LIMIT || CODE_LIMIT - dest < size {
            return Err(ErrorCode::new(1).unwrap());
        }
        if unsafe { flash_rom::write(dest, data.as_ptr(), size) } {
            Ok(())
        } else {
            Err(ErrorCode::new(1).unwrap())
        }
    }

    fn read_flash(&mut self, address: u32, data: &mut [u8]) -> Result<(), ErrorCode> {
        let size = data.len() as u32;
        if size == 0 {
            return Ok(());
        }
        if address >= CODE_LIMIT || CODE_LIMIT - address < size {
            return Err(ErrorCode::new(1).unwrap());
        }
        if unsafe { flash_rom::read(address, data.as_mut_ptr(), size) } {
            Ok(())
        } else {
            Err(ErrorCode::new(1).unwrap())
        }
    }
}
