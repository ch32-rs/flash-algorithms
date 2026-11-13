//! DataFlash algorithm for CH584X.
//!
//! CH584X has 512 KiB of DataFlash with no CPU CODE address. probe-rs exposes
//! it at the program-view address `0x80000..0x100000`; the engine itself is
//! addressed 0-based. CH584X erases in 4 KiB blocks, unlike the 256-byte
//! DataFlash of CH571/CH573/CH581/CH584/CH585.

#![no_std]
#![no_main]

#[path = "../data_flash_584x.rs"]
mod data_flash_584x;

use core::panic::PanicInfo;

#[panic_handler]
fn ph(_: &PanicInfo) -> ! {
    loop {}
}
