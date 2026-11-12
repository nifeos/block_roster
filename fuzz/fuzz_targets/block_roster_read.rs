//! Fuzz the roster reader with an arbitrary page, and walk every device an accepted page lends out.

#![no_main]

use block_roster_contract::{Roster, capacity_of};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|page: &[u8]| {
    let Ok(roster) = Roster::read(page) else {
        return;
    };
    assert!(roster.len() <= capacity_of(page.len()));
    assert_eq!(roster.is_empty(), roster.len() == 0);

    let devices = roster.devices();
    assert_eq!(devices.len(), roster.len());
    let mut seen = 0;
    for (index, device) in devices.enumerate() {
        assert_eq!(roster.get(index), Some(device));
        let _ = device.transport_name();
        seen += 1;
    }
    assert_eq!(seen, roster.len());
    assert_eq!(roster.get(roster.len()), None);
    assert_eq!(roster.get(usize::MAX), None);

    let counted: usize = [u32::MIN, 0, 1, 2, u32::MAX]
        .iter()
        .map(|transport| roster.count_by_transport(*transport))
        .sum();
    assert!(counted <= 5 * roster.len());
});
