//! CRC-32 of a fixed text, timed one time on the board. Replace `TEXT`, `crc32` and the check
//! with your own checks. README.md gives the steps for a library.

#![no_main]
#![no_std]

mod allocator;
mod run;

use libtock::console::Console;
use libtock::runtime::set_main;
use run::{Hexadecimal, Run};

set_main! {main}

const TEXT: &[u8] = b"The quick brown fox jumps over the lazy dog";
const CRC32_EXPECTED: u32 = 0x414F_A339;
const CRC32_BUDGET_MICROSECONDS: u32 = 15;

/// The reflected CRC-32 of IEEE 802.3: polynomial `0xEDB88320`, one bit at a time.
fn crc32(data: &[u8]) -> u32 {
    let mut remainder = u32::MAX;
    for &byte in data {
        remainder ^= u32::from(byte);
        for _ in 0..8 {
            let mask = (remainder & 1).wrapping_neg();
            remainder = (remainder >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !remainder
}

fn main() {
    let Some(mut run) = Run::start(Console::writer()) else {
        return;
    };
    run.check_within_budget(
        "crc32",
        Hexadecimal(CRC32_EXPECTED),
        CRC32_BUDGET_MICROSECONDS,
        || Hexadecimal(crc32(core::hint::black_box(TEXT))),
    );
    run.finish();
}
