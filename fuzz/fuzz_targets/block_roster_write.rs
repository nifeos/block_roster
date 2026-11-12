//! Fuzz the roster writer with an arbitrary page length and device list, and read back what it wrote.
//!
//! The first two input bytes (little-endian) are the page length, taken modulo 1024 so a page holds
//! up to 126 entries. Every eight bytes after them are one device: its ordinal, then its transport.

#![no_main]

use block_roster_contract::{Device, ENTRY_BYTES, HEADER_BYTES, Roster, capacity_of, write};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Some((length, rest)) = data.split_first_chunk::<2>() else {
        return;
    };
    let length = usize::from(u16::from_le_bytes(*length)) % 1024;
    let (chunks, _) = rest.as_chunks::<ENTRY_BYTES>();
    let devices: Vec<Device> = chunks
        .iter()
        .map(|chunk| Device {
            ordinal: u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]),
            transport: u32::from_le_bytes([chunk[4], chunk[5], chunk[6], chunk[7]]),
        })
        .collect();

    let mut page = vec![0xa5u8; length];
    match write(&mut page, &devices) {
        Ok(()) => {
            assert!(devices.len() <= capacity_of(length));
            let roster = Roster::read(&page).expect("a page that was written must read");
            assert_eq!(roster.len(), devices.len());
            assert!(roster.devices().eq(devices.iter().copied()));
        }
        // A page shorter than the header has room for no device, not even for a count of zero.
        Err(_) => assert!(length < HEADER_BYTES || devices.len() > capacity_of(length)),
    }
});
