//! DataFlash algorithm for CH565/CH569.
//!
//! The region is 32 KiB at `0x70000..0x78000`, erased in 4 KiB blocks. It uses
//! the SYS-block flash window and the CH56x command timing (`CTRL=7`), so it is
//! a sibling of the CodeFlash algorithm rather than a CH57x/CH58x/CH59x data
//! binary.

#![no_std]
#![no_main]

#[path = "../data_flash.rs"]
mod data_flash;
