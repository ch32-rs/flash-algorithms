#![no_std]

pub use ch32_metapac::FLASH;
pub use ch32_metapac::metadata::ir::{Block, BlockItem, BlockItemInner, Register};
pub use ch32_metapac::metadata::{METADATA, MemoryRegion, Mode, NvStruct};

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

/// Rounded to 4 so the result is valid for both standard (halfword) and
/// fast-mode (word) OBPG paths.
pub const fn ob_default_writes_size() -> usize {
    let nv = find_ob_struct();
    let block = find_block(nv);
    let mut max_end = 0usize;
    let mut d = 0;
    while d < nv.defaults.len() {
        let (name, _) = nv.defaults[d];
        if let Some(item) = item_by_name(block, name) {
            if let BlockItemInner::Register(reg) = &item.inner {
                let bytes = ((reg.bit_size + 7) / 8) as usize;
                let end = item.byte_offset as usize + bytes;
                if end > max_end {
                    max_end = end;
                }
            }
        }
        d += 1;
    }
    (max_end + 3) & !3
}

/// Fast-mode OBPG does not auto-generate complements, so ch32-data lists
/// complement siblings (e.g. NRDPR) in `defaults` and we write them too.
pub const fn ob_default_writes<const N: usize>() -> [u8; N] {
    let mut buf = [0xFFu8; N];
    let nv = find_ob_struct();
    let block = find_block(nv);
    let mut d = 0;
    while d < nv.defaults.len() {
        let (name, value) = nv.defaults[d];
        if let Some(item) = item_by_name(block, name) {
            if let BlockItemInner::Register(reg) = &item.inner {
                let bytes = ((reg.bit_size + 7) / 8) as usize;
                let start = item.byte_offset as usize;
                if start + bytes <= N {
                    let mut b = 0;
                    while b < bytes {
                        buf[start + b] = ((value as u64 >> (8 * b)) & 0xff) as u8;
                        b += 1;
                    }
                }
            }
        }
        d += 1;
    }
    buf
}

const fn find_ob_struct() -> &'static NvStruct {
    let mut i = 0;
    while i < METADATA.memory.len() {
        let region = &METADATA.memory[i];
        let mut j = 0;
        while j < region.structs.len() {
            if str_eq(region.structs[j].kind, "ob") {
                return &region.structs[j];
            }
            j += 1;
        }
        i += 1;
    }
    panic!("no `ob` struct in METADATA.memory")
}

const fn find_block(nv: &'static NvStruct) -> &'static Block {
    let mut i = 0;
    while i < nv.ir.blocks.len() {
        if str_eq(nv.ir.blocks[i].name, nv.block) {
            return &nv.ir.blocks[i];
        }
        i += 1;
    }
    panic!("OB struct's block name not found in IR")
}

const fn item_by_name(block: &'static Block, name: &str) -> Option<&'static BlockItem> {
    let mut i = 0;
    while i < block.items.len() {
        if str_eq(block.items[i].name, name) {
            return Some(&block.items[i]);
        }
        i += 1;
    }
    None
}
