//! DataFlash algorithm for CH582/CH583 and CH591/CH592.
//!
//! The engine frame address is `0xF0000 + off` (ISP583/ISP592 behaviour; the
//! `0x80000` bit selects the DataFlash bank). CH571/CH573 and CH581/CH584/CH585
//! use [`data`].
//!
//! [`data`]: ../data.rs

#![no_std]
#![no_main]

/// ISP583/ISP592 add the `0x80000` bank bit to every DataFlash frame address.
const OR_MASK: u32 = 0x0008_0000;

#[path = "../data_flash.rs"]
mod data_flash;

use core::panic::PanicInfo;

#[panic_handler]
fn ph(_: &PanicInfo) -> ! {
    loop {}
}
