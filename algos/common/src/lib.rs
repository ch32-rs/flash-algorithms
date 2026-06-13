#![no_std]

pub use ch32_metapac::FLASH;
pub use ch32_metapac::MEMORY_LAYOUT;
pub use ch32_metapac::mem_layout::MemoryRegion;

pub const KEY1: u32 = 0x4567_0123;
pub const KEY2: u32 = 0xCDEF_89AB;

pub const PROGRAM_TIMEOUT_MS: u32 = 1000;
pub const ERASE_TIMEOUT_MS: u32 = 2000;

pub const ERR_NOT_SUPPORTED: core::num::NonZeroU32 = core::num::NonZeroU32::MIN;

pub const fn region(name: &str) -> &'static MemoryRegion {
    match MEMORY_LAYOUT.find(name) {
        Some(r) => r,
        None => panic!("region not found in MEMORY_LAYOUT"),
    }
}

pub const fn fast(r: &MemoryRegion) -> (u32, u32) {
    match (r.page_program_size(), r.fast_load_size()) {
        (Some(page_size), Some(load_size)) => (page_size, load_size),
        _ => panic!("region has no Fast programming mode"),
    }
}

pub const fn standard(r: &MemoryRegion) -> (u32, u32) {
    match (r.erase_size(), r.write_size()) {
        (Some(erase_size), Some(write_size)) => (erase_size, write_size),
        _ => panic!("region has no Standard programming mode"),
    }
}
