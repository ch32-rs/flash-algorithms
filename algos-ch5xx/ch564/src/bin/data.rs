//! DataFlash algorithm for CH564.
//!
//! The region is 32 KiB at `0x30000` (192 KiB config) or `0x70000` (448 KiB
//! config), erased in 4 KiB blocks and programmed byte-wise.

#![no_std]
#![no_main]

#[path = "../flash_rom.rs"]
mod flash_rom;

#[path = "../data_flash.rs"]
mod data_flash;
