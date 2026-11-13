//! DataFlash support for CH564.
//!
//! The 32 KiB DataFlash sits directly after CodeFlash: `0x30000..0x38000` in
//! the 192 KiB configuration and `0x70000..0x78000` in the 448 KiB one. It is
//! erased in 4 KiB blocks and programmed byte-wise, like the other CH5xx data
//! regions. The configuration byte is read by [`Algo::new`] so a single binary
//! serves both target variants.

use flash_algorithm::{ErrorCode, FlashAlgorithm, Function, algorithm};

use crate::flash_rom;

const DATA_BASE_256K: u32 = 0x0003_0000;
const DATA_BASE_512K: u32 = 0x0007_0000;
const DATA_SIZE: u32 = 0x0000_8000;
const ERASE_SIZE: u32 = 0x1000;
const PAGE_SIZE: u32 = 0x100;
const PROGRAM_TIMEOUT_MS: u32 = 500;
const ERASE_TIMEOUT_MS: u32 = 1000;

struct Algo {
    base: u32,
}

algorithm!(Algo, {
    device_name: "CH564 DataFlash",
    device_type: DeviceType::Onchip,
    flash_address: DATA_BASE_256K,
    flash_size: DATA_SIZE,
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
        let Some(is_512k) = (unsafe { flash_rom::config_512k() }) else {
            return Err(ErrorCode::new(1).unwrap());
        };
        let base = if is_512k {
            DATA_BASE_512K
        } else {
            DATA_BASE_256K
        };
        if addr != base {
            return Err(ErrorCode::new(1).unwrap());
        }
        Ok(Self { base })
    }

    fn erase_all(&mut self) -> Result<(), ErrorCode> {
        if unsafe { flash_rom::erase_range(self.base, self.base + DATA_SIZE) } {
            Ok(())
        } else {
            Err(ErrorCode::new(1).unwrap())
        }
    }

    fn erase_sector(&mut self, sector_addr: u32) -> Result<(), ErrorCode> {
        let aligned = sector_addr & !(ERASE_SIZE - 1);
        if aligned < self.base || self.base + DATA_SIZE - aligned < ERASE_SIZE {
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
        if dest < self.base || self.base + DATA_SIZE - dest < size {
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
        if address < self.base || self.base + DATA_SIZE - address < size {
            return Err(ErrorCode::new(1).unwrap());
        }
        if unsafe { flash_rom::read(address, data.as_mut_ptr(), size) } {
            Ok(())
        } else {
            Err(ErrorCode::new(1).unwrap())
        }
    }
}
