#![no_std]

pub use ch32_metapac::FLASH;
pub use ch32_metapac::metadata::{METADATA, MemoryRegion, Mode};

pub const KEY1: u32 = 0x4567_0123;
pub const KEY2: u32 = 0xCDEF_89AB;

pub const PROGRAM_TIMEOUT_MS: u32 = 1000;
pub const ERASE_TIMEOUT_MS: u32 = 2000;

pub const ERR_NOT_SUPPORTED: core::num::NonZeroU32 = core::num::NonZeroU32::MIN;

pub const fn str_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    let mut i = 0;
    while i < a.len() {
        if a[i] != b[i] {
            return false;
        }
        i += 1;
    }
    true
}

pub const fn region(name: &str) -> &'static MemoryRegion {
    let mem = METADATA.memory;
    let mut i = 0;
    while i < mem.len() {
        if str_eq(mem[i].name, name) {
            return &mem[i];
        }
        i += 1;
    }
    panic!("region not found in METADATA.memory")
}

pub const fn fast(r: &MemoryRegion) -> (u32, u32) {
    let mut i = 0;
    while i < r.modes.len() {
        if let Mode::Fast {
            page_size,
            load_size,
        } = r.modes[i]
        {
            return (page_size, load_size);
        }
        i += 1;
    }
    panic!("region has no Fast programming mode")
}

pub const fn standard(r: &MemoryRegion) -> (u32, u32) {
    let mut i = 0;
    while i < r.modes.len() {
        if let Mode::Standard {
            erase_size,
            write_size,
        } = r.modes[i]
        {
            return (erase_size, write_size);
        }
        i += 1;
    }
    panic!("region has no Standard programming mode")
}
