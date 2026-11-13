//! CodeFlash algorithm for CH595/CH596.
//!
//! Both parts have 240 KiB of user CodeFlash and no user DataFlash.

#![no_std]
#![no_main]

#[path = "../flash_rom.rs"]
mod flash_rom;

#[path = "../code_flash.rs"]
mod code_flash;
