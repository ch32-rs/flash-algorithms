//! CodeFlash algorithm for CH584X.
//!
//! CH584X has 480 KiB of CodeFlash at `0x0..0x77FFF`. Its engine frame address
//! is `addr + 0x80000`, unlike CH584/CH585, where the CPU CodeFlash address is
//! also the engine address. The algorithm is selected explicitly with
//! `--chip CH584X` and verifies `(*(uint32_t *)0x7F024 & 0x0F) == 0x08`.

#![no_std]
#![no_main]

#[path = "../code_flash_584x.rs"]
mod code_flash_584x;

use core::panic::PanicInfo;

#[panic_handler]
fn ph(_: &PanicInfo) -> ! {
    loop {}
}
