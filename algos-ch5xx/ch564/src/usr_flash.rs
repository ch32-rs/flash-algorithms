//! CodeFlash support for CH564.
//!
//! CH564 has two memory configurations, selected by InfoFlash `0x3007` bit 5:
//! 192 KiB CodeFlash at `0..0x30000`, or 448 KiB at `0..0x70000`. The
//! DataFlash follows the CodeFlash in both maps (32 KiB at `0x30000` or
//! `0x70000`). The official `FLASH_ROMA_*` API reads that config byte through
//! command `0x48`; the algorithm does the same in [`Algo::new`], so a single
//! binary serves both target variants.

use flash_algorithm::{ErrorCode, FlashAlgorithm, Function, algorithm};

use crate::flash_rom;

const CODE_LIMIT_256K: u32 = 0x0003_0000;
const CODE_LIMIT_512K: u32 = 0x0007_0000;
const ERASE_SIZE: u32 = 0x1000;
const PAGE_SIZE: u32 = 0x100;
const PROGRAM_TIMEOUT_MS: u32 = 500;
const ERASE_TIMEOUT_MS: u32 = 1000;

struct Algo {
    limit: u32,
}

algorithm!(Algo, {
    device_name: "CH564 CodeFlash",
    device_type: DeviceType::Onchip,
    flash_address: 0,
    flash_size: CODE_LIMIT_512K,
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
        let limit = if is_512k {
            CODE_LIMIT_512K
        } else {
            CODE_LIMIT_256K
        };
        if addr >= limit {
            return Err(ErrorCode::new(1).unwrap());
        }
        Ok(Self { limit })
    }

    fn erase_all(&mut self) -> Result<(), ErrorCode> {
        if unsafe { flash_rom::erase_range(0, self.limit) } {
            Ok(())
        } else {
            Err(ErrorCode::new(1).unwrap())
        }
    }

    fn erase_sector(&mut self, sector_addr: u32) -> Result<(), ErrorCode> {
        let aligned = sector_addr & !(ERASE_SIZE - 1);
        if aligned >= self.limit || self.limit - aligned < ERASE_SIZE {
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
        if dest >= self.limit || self.limit - dest < size {
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
        if address >= self.limit || self.limit - address < size {
            return Err(ErrorCode::new(1).unwrap());
        }
        if unsafe { flash_rom::read(address, data.as_mut_ptr(), size) } {
            Ok(())
        } else {
            Err(ErrorCode::new(1).unwrap())
        }
    }
}
