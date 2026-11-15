//! DataFlash algorithm for CH571/CH573, CH581/CH584/CH585.
//!
//! The engine frame address is `0x70000 + off` (ISP573/ISP585 behaviour; the engine
//! frame address has no `0x80000` bit). CH582/CH583/CH591/CH592 need [`data-0x80000`].
//!
//! [`data-0x80000`]: ../data-0x80000.rs

#![no_std]
#![no_main]

/// No `0x80000` frame bit for this generation.
const OR_MASK: u32 = 0;

#[path = "../data_flash.rs"]
mod data_flash;

use core::panic::PanicInfo;

#[panic_handler]
fn ph(_: &PanicInfo) -> ! {
    loop {}
}
