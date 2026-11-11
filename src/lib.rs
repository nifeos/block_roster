//! **What drives are attached**: the roster page, and the authority it is not. `lsblk` reads an
//! ambient `/sys/block`; here the kernel writes this page once at wiring time and maps it read-only
//! into exactly the programs granted it, so a program without it has nowhere to look.
//!
//! The roster says a device exists, its bus and its ordinal, and carries no capacity: size is
//! learned by asking the device, which takes the endpoint. Listing the drives and holding one are
//! different authorities, so the roster is a read-only mapping and not a verb: it has no server and
//! needs no seqlock.

//! The page, little-endian, fixed width and without pointers:
//! ```text
//!   0..8     MAGIC
//!   8..12    count: how many entries follow
//!   12..16   capacity: how many entries this page has room for
//!   16..     entries, ENTRY_BYTES each
//!   an entry:
//!   0..4     ordinal: which device of this transport, 0-based
//!   4..8     transport: one of the TRANSPORT_ kinds below
//! ```
//!
//! [`Roster::read`] checks the header before it lends out an entry: the page holds the header, it
//! starts with [`MAGIC`], and the count fits both the page and the capacity word, which in turn fits
//! the page. Each refusal is its own [`ReadError`].
//!
//! # What a reader may trust
//!
//! The header checks are all `read` does. The ordinal and the transport of each entry are numbers
//! the writer chose, and nothing here bounds them: a reader that indexes a table of its own by
//! [`Device::ordinal`] bounds the index itself, and a transport it does not know has no name
//! ([`Device::transport_name`] returns `None`). The page is a snapshot of the moment the writer laid it
//! out. It does not change while a program holds it, and it says nothing of a device attached
//! later.
//!
//! # Identity
//!
//! The page's identity is frozen: [`INTERFACE_ID`] is 5, [`VERSION`] is 1 and [`MAGIC`] is the
//! bytes `blkrosv1`. The id stays 5 for good. A layout break is a new version, which re-mints the
//! magic in the same change; it is never a new id, and a retired value is never reused.

#![no_std]
#![forbid(unsafe_code)]

#[cfg(doctest)]
#[doc = include_str!("../README.md")]
struct ReadmeExample;

/// `"blkrosv1"` little-endian, at offset 0. A page that does not start with this is not a roster,
/// and in particular a zeroed frame is not an empty one. The digits are the page layout's version
/// ([`VERSION`]); a bump re-mints both in one change.
pub const MAGIC: u64 = u64::from_le_bytes(*b"blkrosv1");

/// The page layout's version, the digits [`MAGIC`] carries. A break bumps this and re-mints the
/// magic in the same change, and `CHANGELOG.md` says what the new layout is.
pub const VERSION: u32 = 1;

/// This interface's identity: 5, naming the block roster page. The number is the crate's to keep
/// across a rename and no other interface ever reuses it; the ids of the interfaces a program links
/// must all differ. A program that links this interface declares the pair of this id and
/// [`VERSION`].
pub const INTERFACE_ID: u64 = 5;

/// Bytes per entry. Two `u32`s, and the fixed width is what lets a reader index without parsing.
pub const ENTRY_BYTES: usize = 8;

/// Where the first entry starts: magic, count, capacity.
pub const HEADER_BYTES: usize = 16;

#[doc(alias = "virtio-mmio")]
/// A virtio-mmio device: the ordinal is its position among block devices on the mmio bus, counted
/// the way the kernel's own scan counts them.
pub const TRANSPORT_VIRTIO_MEMORY_MAPPED_IO: u32 = 0;

#[doc(alias = "virtio-pci")]
/// A virtio-blk function on the `PCIe` bus (the virtio specification's Peripheral Component Interconnect (PCI) transport).
pub const TRANSPORT_VIRTIO_PCI: u32 = 1;

#[doc(alias = "nvme")]
/// An `NVMe` controller on the `PCIe` bus, found by its class code: the ordinal is its position among
/// `NVMe` controllers in the kernel's bus walk, so ordinal 0 is the first
/// controller the kernel finds.
pub const TRANSPORT_NON_VOLATILE_MEMORY_EXPRESS: u32 = 2;

/// One attached block device: which bus, and which one of them.
///
/// Deliberately not a handle. Holding this tells you a device exists; it does not let you touch it,
/// and there is no method here that turns one into the other. The kernel decides which program gets
/// which device's endpoint, at wiring time, and nothing a program computes from a roster can change
/// that.
///
/// Both fields are the writer's numbers, unchecked: see "What a reader may trust" in the crate
/// documentation.
///
/// The derived order sorts by transport and then by ordinal, so a sorted listing groups the
/// devices by bus. The order of the fields below is that sort order, and it is published behavior:
/// moving a field changes how every caller's sorted listing reads. The page layout does not follow
/// the field order, since [`write()`] and [`Roster::get`] name each field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Device {
    /// [`TRANSPORT_VIRTIO_MEMORY_MAPPED_IO`], [`TRANSPORT_VIRTIO_PCI`] or
    /// [`TRANSPORT_NON_VOLATILE_MEMORY_EXPRESS`].
    pub transport: u32,
    /// Which block device of this transport, 0-based, in the order the kernel's scan finds them.
    pub ordinal: u32,
}

impl Device {
    /// The transport's name, for a listing a person reads. `None` for a number this build does not
    /// know, which is what a reader of a page written by a newer kernel would see.
    ///
    /// The printed names are the ones the outside world already uses rather than the identifiers:
    /// the virtio specification's two transports, and `nvme`, which is QEMU's device model, Linux's
    /// driver and device prefix, and what diagnostics for that hardware print.
    pub const fn transport_name(&self) -> Option<&'static str> {
        match self.transport {
            TRANSPORT_VIRTIO_MEMORY_MAPPED_IO => Some("virtio-mmio"),
            TRANSPORT_VIRTIO_PCI => Some("virtio-pci"),
            TRANSPORT_NON_VOLATILE_MEMORY_EXPRESS => Some("nvme"),
            _ => None,
        }
    }

    /// The device one entry of a page holds: the ordinal in its first four bytes, the transport
    /// in the next four.
    const fn from_entry(entry: &[u8; ENTRY_BYTES]) -> Device {
        Device {
            transport: u32::from_le_bytes([entry[4], entry[5], entry[6], entry[7]]),
            ordinal: u32::from_le_bytes([entry[0], entry[1], entry[2], entry[3]]),
        }
    }
}

/// How many entries a page of `len` bytes can hold.
pub const fn capacity_of(len: usize) -> usize {
    if len < HEADER_BYTES {
        0
    } else {
        (len - HEADER_BYTES) / ENTRY_BYTES
    }
}

/// Lay a roster into `page`. Refuses with [`TooManyDevices`], and leaves `page` untouched, when the
/// page cannot hold the header and `devices`: a page shorter than [`HEADER_BYTES`] holds no roster
/// at all, not even an empty one. `write` never panics, whatever the lengths.
///
/// The count and capacity words are `u32`, so a page with room for more than `u32::MAX` entries
/// records a capacity of `u32::MAX` and takes at most that many devices.
///
/// One writer calls this, once, before any program can see the page. See the crate header for what
/// that buys the reader.
///
/// # Examples
///
/// ```
/// # use block_roster_contract::{Device, Roster, TRANSPORT_NON_VOLATILE_MEMORY_EXPRESS};
/// # use block_roster_contract::{TRANSPORT_VIRTIO_MEMORY_MAPPED_IO, TRANSPORT_VIRTIO_PCI};
/// let mut page = [0u8; 4096];
/// let devices = [
///     Device { ordinal: 0, transport: TRANSPORT_VIRTIO_MEMORY_MAPPED_IO },
///     Device { ordinal: 0, transport: TRANSPORT_VIRTIO_PCI },
///     Device { ordinal: 0, transport: TRANSPORT_NON_VOLATILE_MEMORY_EXPRESS },
/// ];
/// block_roster_contract::write(&mut page, &devices).unwrap();
///
/// let roster = Roster::read(&page).unwrap();
/// assert_eq!(roster.len(), 3);
/// assert_eq!(roster.get(2).unwrap().transport_name(), Some("nvme"));
/// ```
pub fn write(page: &mut [u8], devices: &[Device]) -> Result<(), TooManyDevices> {
    if page.len() < HEADER_BYTES {
        return Err(TooManyDevices);
    }
    let capacity = u32::try_from(capacity_of(page.len())).unwrap_or(u32::MAX);
    let Ok(count) = u32::try_from(devices.len()) else {
        return Err(TooManyDevices);
    };
    if count > capacity {
        return Err(TooManyDevices);
    }
    page[..8].copy_from_slice(&MAGIC.to_le_bytes());
    page[8..12].copy_from_slice(&count.to_le_bytes());
    page[12..16].copy_from_slice(&capacity.to_le_bytes());
    for (index, device) in devices.iter().enumerate() {
        let at = HEADER_BYTES + index * ENTRY_BYTES;
        page[at..at + 4].copy_from_slice(&device.ordinal.to_le_bytes());
        page[at + 4..at + 8].copy_from_slice(&device.transport.to_le_bytes());
    }
    Ok(())
}

/// More devices than the page has room for, which includes a page too short for the header: it has
/// room for no device, not even for the count of zero. The kernel treats this as a wiring bug
/// rather than a runtime condition: a machine with more disks than a page can name is a machine
/// this design has to be told about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TooManyDevices;

impl core::fmt::Display for TooManyDevices {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("the page has no room for the roster header and these devices")
    }
}

impl core::error::Error for TooManyDevices {}

/// Why [`Roster::read`] refused a page. Each refusal points at a different fault, so a reader can
/// report which one it met without parsing the page itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ReadError {
    /// The page is shorter than [`HEADER_BYTES`], so it cannot hold even an empty roster: the
    /// mapping is shorter than the page it stands for.
    TooShort,
    /// The page does not start with [`MAGIC`]. A frame nobody wrote, a page of another kind, or a
    /// roster of another [`VERSION`].
    NotARoster,
    /// The count word names more entries than the page has room for: the writer is broken, or
    /// the mapping is shorter than the page.
    CountBeyondPage,
    /// The capacity word names more entries than the page has room for: the mapping is shorter
    /// than the page the writer laid out.
    CapacityBeyondPage,
    /// The count word names more entries than the page's own capacity word: the writer is broken.
    CountBeyondCapacity,
}

impl core::fmt::Display for ReadError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            ReadError::TooShort => "the page is shorter than the roster header",
            ReadError::NotARoster => "the page does not start with the roster magic",
            ReadError::CountBeyondPage => "the roster's count runs past the end of the page",
            ReadError::CapacityBeyondPage => {
                "the roster's capacity word claims more room than the page has"
            }
            ReadError::CountBeyondCapacity => {
                "the roster's count is larger than its own capacity word"
            }
        })
    }
}

impl core::error::Error for ReadError {}

/// A roster page a program holds, borrowed rather than copied.
#[derive(Debug, Clone, Copy)]
pub struct Roster<'a> {
    /// The counted entries and nothing past them.
    entries: &'a [[u8; ENTRY_BYTES]],
}

impl<'a> Roster<'a> {
    /// Read a mapped page, or say why it is not a roster: [`ReadError`] names each refusal.
    ///
    /// `read` checks the header against the page in this order: the page holds the header
    /// ([`ReadError::TooShort`]), it starts with [`MAGIC`] ([`ReadError::NotARoster`]), the count
    /// fits the page ([`ReadError::CountBeyondPage`]), the capacity word fits the page
    /// ([`ReadError::CapacityBeyondPage`]), and the count fits the capacity word
    /// ([`ReadError::CountBeyondCapacity`]). A page longer than the capacity word says is
    /// accepted: a reader may map more than the writer laid out.
    ///
    /// Every refusal matters for the same reason: this page arrives from outside the program, and a
    /// reader that trusted a header it did not check would index past its own mapping.
    pub fn read(page: &'a [u8]) -> Result<Roster<'a>, ReadError> {
        if page.len() < HEADER_BYTES {
            return Err(ReadError::TooShort);
        }
        if page[..8] != MAGIC.to_le_bytes() {
            return Err(ReadError::NotARoster);
        }
        let word =
            |at: usize| u32::from_le_bytes([page[at], page[at + 1], page[at + 2], page[at + 3]]);
        let room = capacity_of(page.len());
        let count = word(8) as usize;
        let capacity = word(12) as usize;
        if count > room {
            return Err(ReadError::CountBeyondPage);
        }
        if capacity > room {
            return Err(ReadError::CapacityBeyondPage);
        }
        if count > capacity {
            return Err(ReadError::CountBeyondCapacity);
        }
        let (entries, _) = page[HEADER_BYTES..].as_chunks::<ENTRY_BYTES>();
        Ok(Roster {
            entries: &entries[..count],
        })
    }

    /// How many devices are attached.
    pub const fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the machine has no block devices at all. A real answer, not an error: a diskless
    /// boot is a thing a machine can do.
    pub const fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The device at `index`, counting from 0, or `None` past the end.
    pub fn get(&self, index: usize) -> Option<Device> {
        self.entries.get(index).map(Device::from_entry)
    }

    /// Every device, in the order the kernel found them. The iterator knows its length, which is
    /// [`Roster::len`].
    pub fn devices(&self) -> impl ExactSizeIterator<Item = Device> + '_ {
        self.entries.iter().map(Device::from_entry)
    }

    /// How many of the devices came off `transport`.
    pub fn count_by_transport(&self, transport: u32) -> usize {
        self.devices()
            .filter(|device| device.transport == transport)
            .count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> [Device; 4] {
        [
            Device {
                ordinal: 0,
                transport: TRANSPORT_VIRTIO_MEMORY_MAPPED_IO,
            },
            Device {
                ordinal: 1,
                transport: TRANSPORT_VIRTIO_MEMORY_MAPPED_IO,
            },
            Device {
                ordinal: 2,
                transport: TRANSPORT_VIRTIO_MEMORY_MAPPED_IO,
            },
            Device {
                ordinal: 0,
                transport: TRANSPORT_VIRTIO_PCI,
            },
        ]
    }

    #[test]
    fn a_roster_survives_the_round_trip() {
        let mut page = [0u8; 4096];
        write(&mut page, &sample()).unwrap();
        let roster = Roster::read(&page).unwrap();
        assert_eq!(roster.len(), 4);
        assert!(!roster.is_empty());
        for (index, want) in sample().iter().enumerate() {
            assert_eq!(roster.get(index).unwrap(), *want);
        }
        assert_eq!(
            roster.get(4),
            None,
            "past the end is None, not the next entry"
        );
        assert_eq!(
            roster.count_by_transport(TRANSPORT_VIRTIO_MEMORY_MAPPED_IO),
            3
        );
        assert_eq!(roster.count_by_transport(TRANSPORT_VIRTIO_PCI), 1);
        assert_eq!(roster.devices().len(), 4);
        assert!(roster.devices().eq(sample()));
    }

    /// The refusal that costs nothing and buys the most: a frame nobody wrote is not a machine
    /// with no disks. Reading it as "zero devices" would be a lie that looks like a fact, and it
    /// is exactly what a wrongly wired mapping produces.
    #[test]
    fn a_zeroed_page_is_not_an_empty_roster() {
        let page = [0u8; 4096];
        assert_eq!(Roster::read(&page).err(), Some(ReadError::NotARoster));

        // Whereas a roster the kernel deliberately wrote empty reads as empty.
        let mut page = [0u8; 4096];
        write(&mut page, &[]).unwrap();
        let roster = Roster::read(&page).unwrap();
        assert!(roster.is_empty());
        assert_eq!(roster.get(0), None);
    }

    /// A count larger than the page can hold is refused rather than trusted. The reader is a
    /// `no_std` program indexing a mapping; believing this number is how it walks off the end.
    #[test]
    fn a_lying_count_is_refused() {
        let mut page = [0u8; 64]; // room for six entries
        write(&mut page, &sample()).unwrap();
        assert!(Roster::read(&page).is_ok());
        page[8..12].copy_from_slice(&7u32.to_le_bytes());
        assert_eq!(Roster::read(&page).err(), Some(ReadError::CountBeyondPage));
        // And exactly at capacity is fine, because the boundary is the interesting one.
        page[8..12].copy_from_slice(&6u32.to_le_bytes());
        assert!(Roster::read(&page).is_ok());
    }

    /// A capacity word larger than the page has room for means the reader holds less of the page
    /// than the writer laid out, even when the count still fits.
    #[test]
    fn a_capacity_word_past_the_page_is_refused() {
        let mut page = [0u8; 64]; // room for six entries
        write(&mut page, &sample()).unwrap();
        page[12..16].copy_from_slice(&7u32.to_le_bytes());
        assert_eq!(
            Roster::read(&page).err(),
            Some(ReadError::CapacityBeyondPage)
        );
        // The writer's own page, read through a mapping one entry short of it.
        write(&mut page, &sample()).unwrap();
        assert_eq!(
            Roster::read(&page[..56]).err(),
            Some(ReadError::CapacityBeyondPage)
        );
        page[12..16].copy_from_slice(&6u32.to_le_bytes());
        assert!(Roster::read(&page).is_ok());
    }

    /// A count larger than the page's own capacity word came from a broken writer, even when the
    /// page has room for the entries.
    #[test]
    fn a_count_past_the_capacity_word_is_refused() {
        let mut page = [0u8; 64]; // room for six entries
        write(&mut page, &sample()).unwrap();
        page[12..16].copy_from_slice(&3u32.to_le_bytes());
        assert_eq!(
            Roster::read(&page).err(),
            Some(ReadError::CountBeyondCapacity)
        );
        page[12..16].copy_from_slice(&4u32.to_le_bytes());
        assert_eq!(Roster::read(&page).map(|roster| roster.len()), Ok(4));
    }

    /// A reader may map more than the writer laid out: a capacity word smaller than the room the
    /// page has is accepted, and the entries past the count are not read.
    #[test]
    fn a_page_longer_than_its_capacity_word_reads() {
        let mut page = [0xA5u8; 64];
        write(&mut page[..HEADER_BYTES + 4 * ENTRY_BYTES], &sample()).unwrap();
        let roster = Roster::read(&page).unwrap();
        assert_eq!(roster.len(), 4);
        assert_eq!(roster.get(4), None);
        assert!(roster.devices().eq(sample()));
    }

    /// Each refusal says something different, so a log line names the fault.
    #[test]
    fn every_refusal_has_its_own_message() {
        let errors = [
            ReadError::TooShort,
            ReadError::NotARoster,
            ReadError::CountBeyondPage,
            ReadError::CapacityBeyondPage,
            ReadError::CountBeyondCapacity,
        ];
        let mut messages = [[0u8; 128]; 5];
        let mut lengths = [0usize; 5];
        for (index, error) in errors.iter().enumerate() {
            lengths[index] = render(error, &mut messages[index]);
            assert!(lengths[index] > 0);
        }
        for first in 0..errors.len() {
            for second in first + 1..errors.len() {
                assert_ne!(
                    messages[first][..lengths[first]],
                    messages[second][..lengths[second]]
                );
            }
        }
        let mut message = [0u8; 128];
        assert!(render(&TooManyDevices, &mut message) > 0);
        fn is_error<E: core::error::Error>(_: &E) {}
        is_error(&TooManyDevices);
        is_error(&ReadError::TooShort);
    }

    /// Writes `value`'s `Display` text into `buffer` and returns its length, with no allocator.
    fn render(value: &dyn core::fmt::Display, buffer: &mut [u8]) -> usize {
        struct Cursor<'b> {
            buffer: &'b mut [u8],
            used_bytes: usize,
        }
        impl core::fmt::Write for Cursor<'_> {
            fn write_str(&mut self, text: &str) -> core::fmt::Result {
                let end = self.used_bytes + text.len();
                self.buffer
                    .get_mut(self.used_bytes..end)
                    .ok_or(core::fmt::Error)?
                    .copy_from_slice(text.as_bytes());
                self.used_bytes = end;
                Ok(())
            }
        }
        let mut cursor = Cursor {
            buffer,
            used_bytes: 0,
        };
        core::fmt::write(&mut cursor, format_args!("{value}")).unwrap();
        cursor.used_bytes
    }

    /// The derived order is published behavior: by transport first, then by ordinal, so a sorted
    /// listing groups the devices by bus. Moving a field of `Device` fails this test.
    #[test]
    fn devices_sort_by_transport_then_ordinal() {
        let mut devices = [
            Device {
                transport: TRANSPORT_VIRTIO_PCI,
                ordinal: 0,
            },
            Device {
                transport: TRANSPORT_VIRTIO_MEMORY_MAPPED_IO,
                ordinal: 1,
            },
            Device {
                transport: TRANSPORT_NON_VOLATILE_MEMORY_EXPRESS,
                ordinal: 0,
            },
            Device {
                transport: TRANSPORT_VIRTIO_MEMORY_MAPPED_IO,
                ordinal: 0,
            },
        ];
        devices.sort();
        let order = devices.map(|device| (device.transport, device.ordinal));
        assert_eq!(order, [(0, 0), (0, 1), (1, 0), (2, 0)]);
    }

    /// Each half of an entry lands in its own field, read from the bytes the layout names.
    #[test]
    fn an_entry_reads_its_ordinal_then_its_transport() {
        let mut page = [0u8; HEADER_BYTES + ENTRY_BYTES];
        write(
            &mut page,
            &[Device {
                transport: 0,
                ordinal: 0,
            }],
        )
        .unwrap();
        page[HEADER_BYTES..].copy_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8]);
        let device = Roster::read(&page).unwrap().get(0).unwrap();
        assert_eq!(device.ordinal, 0x0403_0201);
        assert_eq!(device.transport, 0x0807_0605);
    }

    #[test]
    fn a_page_too_small_for_its_devices_is_refused_at_write() {
        let mut page = [0u8; HEADER_BYTES + ENTRY_BYTES]; // room for one
        assert_eq!(write(&mut page, &sample()), Err(TooManyDevices));
        assert_eq!(write(&mut page, &sample()[..1]), Ok(()));
        assert_eq!(capacity_of(0), 0, "a page with no header holds nothing");
        assert_eq!(capacity_of(HEADER_BYTES), 0);
    }

    /// A page shorter than the header holds no roster, not even an empty one, so `write` refuses it
    /// rather than slicing past its end. Every length from zero to two entries past the header,
    /// with zero to two devices, either fits or is refused, and a refused page is left as it was.
    #[test]
    fn write_refuses_a_page_too_short_for_its_header_and_never_panics() {
        let devices = sample();
        for len in 0..=HEADER_BYTES + 2 * ENTRY_BYTES {
            for count in 0..=2 {
                let mut page = [0xA5u8; HEADER_BYTES + 2 * ENTRY_BYTES];
                let fits = len >= HEADER_BYTES && count <= capacity_of(len);
                let result = write(&mut page[..len], &devices[..count]);
                assert_eq!(result.is_ok(), fits, "length {len}, {count} devices");
                if fits {
                    assert_eq!(
                        Roster::read(&page[..len]).map(|roster| roster.len()),
                        Ok(count)
                    );
                } else {
                    assert!(
                        page.iter().all(|&byte| byte == 0xA5),
                        "a refused page is untouched"
                    );
                }
            }
        }
        assert_eq!(write(&mut [], &[]), Err(TooManyDevices));
        assert_eq!(
            write(&mut [0u8; HEADER_BYTES - 1], &[]),
            Err(TooManyDevices)
        );
    }

    #[test]
    fn transports_are_named_and_an_unknown_one_is_not_guessed() {
        assert_eq!(
            Device {
                ordinal: 0,
                transport: TRANSPORT_VIRTIO_MEMORY_MAPPED_IO
            }
            .transport_name(),
            Some("virtio-mmio"),
        );
        assert_eq!(
            Device {
                ordinal: 0,
                transport: TRANSPORT_VIRTIO_PCI
            }
            .transport_name(),
            Some("virtio-pci"),
        );
        assert_eq!(
            Device {
                ordinal: 0,
                transport: TRANSPORT_NON_VOLATILE_MEMORY_EXPRESS
            }
            .transport_name(),
            Some("nvme"),
        );
        assert_eq!(
            Device {
                ordinal: 0,
                transport: 99
            }
            .transport_name(),
            None,
        );
    }

    /// The kinds are wire values a `no_std` reader built from another commit compares against, so
    /// an identifier may move and its number may not. Pinned as literals because a test written
    /// against the constants would follow a renumbering instead of catching it.
    #[test]
    fn the_transport_kinds_keep_their_wire_values() {
        assert_eq!(TRANSPORT_VIRTIO_MEMORY_MAPPED_IO, 0);
        assert_eq!(TRANSPORT_VIRTIO_PCI, 1);
        assert_eq!(TRANSPORT_NON_VOLATILE_MEMORY_EXPRESS, 2);
    }

    /// The magic is a fixed byte string on disk, not whatever `u64::from_le_bytes` happens to
    /// produce today. Spelling it out is what makes a page written by one build readable by
    /// another built on a different endianness.
    #[test]
    fn the_identity_keeps_its_frozen_values() {
        assert_eq!(INTERFACE_ID, 5);
        assert_eq!(VERSION, 1);
        assert_eq!(MAGIC.to_le_bytes(), *b"blkrosv1");
    }

    #[test]
    fn the_magic_is_the_bytes_it_says_it_is() {
        let mut page = [0u8; 4096];
        write(&mut page, &[]).unwrap();
        assert_eq!(&page[..8], b"blkrosv1");
    }

    /// A page of exactly `HEADER_BYTES` is the smallest roster there is: room for the header,
    /// zero devices, and it must read back as that rather than being refused as short. A mutation run
    /// showed `read`'s length guard could rot from `<` to `<=` unnoticed because every test used a
    /// whole 4096-byte page.
    #[test]
    fn a_header_only_page_is_an_empty_roster_not_a_short_one() {
        let mut page = [0u8; HEADER_BYTES];
        write(&mut page, &[]).unwrap();
        let roster = Roster::read(&page).expect("header-only is a valid roster");
        assert_eq!(roster.devices().count(), 0);
        assert_eq!(capacity_of(HEADER_BYTES), 0);
        // One byte shorter really is short.
        assert_eq!(
            Roster::read(&page[..HEADER_BYTES - 1]).err(),
            Some(ReadError::TooShort)
        );
    }
}

/// Machine-checked proofs, run with `cargo kani`. The bounds are small on purpose: a page of up to
/// three entries exercises every branch of the header checks and the entry indexing.
#[cfg(kani)]
mod verification {
    use super::*;

    /// The page the harnesses build: the header and room for three entries.
    /// A literal, not `HEADER_BYTES + 3 * ENTRY_BYTES`: arithmetic in a constant has no function
    /// name, so the mutation census cannot attribute it to this module. The first harness checks it.
    const PAGE: usize = 40;

    /// Falsification: replayable `falsifications/verification.read_is_total_and_every_counted_entry_is_gettable.patch`
    /// **`Roster::read` is total on any bytes**: for any page of up to `PAGE` bytes, whatever its
    /// contents and length, `read` neither panics nor indexes out of bounds, and when it accepts the
    /// page its `len()` is the count the header names, every one of those entries can be fetched,
    /// `devices()` yields exactly that many, and the entry after the last cannot be fetched.
    #[kani::proof]
    #[kani::unwind(9)]
    fn read_is_total_and_every_counted_entry_is_gettable() {
        assert_eq!(PAGE, HEADER_BYTES + 3 * ENTRY_BYTES);
        let bytes: [u8; PAGE] = kani::any();
        let len: usize = kani::any();
        kani::assume(len <= PAGE);

        if let Ok(roster) = Roster::read(&bytes[..len]) {
            let count = u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]);
            assert_eq!(
                roster.len(),
                count as usize,
                "the roster is the count the header names"
            );
            assert_eq!(roster.devices().len(), roster.len());
            let mut index = 0;
            while index < roster.len() {
                assert!(roster.get(index).is_some(), "a counted entry is missing");
                index += 1;
            }
            assert!(roster.get(roster.len()).is_none());
        }
    }

    /// Falsification: replayable `falsifications/verification.write_then_read_returns_the_same_devices_in_order.patch`
    /// **A written roster reads back as written**: for any list of up to three devices, `write`
    /// into a page that fits and then `read` returns the same devices in the same order, and no
    /// more.
    #[kani::proof]
    #[kani::unwind(9)]
    fn write_then_read_returns_the_same_devices_in_order() {
        let devices: [Device; 3] = [
            Device {
                ordinal: kani::any(),
                transport: kani::any(),
            },
            Device {
                ordinal: kani::any(),
                transport: kani::any(),
            },
            Device {
                ordinal: kani::any(),
                transport: kani::any(),
            },
        ];
        let count: usize = kani::any();
        kani::assume(count <= devices.len());

        let mut page = [0u8; PAGE];
        assert_eq!(write(&mut page, &devices[..count]), Ok(()));

        let roster = Roster::read(&page).expect("a written page reads");
        assert_eq!(roster.len(), count);
        let mut index = 0;
        while index < count {
            assert_eq!(roster.get(index), Some(devices[index]));
            index += 1;
        }
        assert!(roster.get(count).is_none());
    }

    /// Falsification: replayable `falsifications/verification.write_is_total_on_any_page_length.patch`
    /// **`write` is total on any page length**: for any page of up to `PAGE` bytes and any list of
    /// up to three devices, `write` neither panics nor indexes out of bounds. It succeeds exactly
    /// when the page holds the header and the devices, and a page it accepted reads back with that
    /// many devices.
    #[kani::proof]
    #[kani::unwind(9)]
    fn write_is_total_on_any_page_length() {
        let devices: [Device; 3] = [
            Device {
                ordinal: kani::any(),
                transport: kani::any(),
            },
            Device {
                ordinal: kani::any(),
                transport: kani::any(),
            },
            Device {
                ordinal: kani::any(),
                transport: kani::any(),
            },
        ];
        let count: usize = kani::any();
        kani::assume(count <= devices.len());
        let len: usize = kani::any();
        kani::assume(len <= PAGE);

        let mut page = [0u8; PAGE];
        let fits = len >= HEADER_BYTES && count <= capacity_of(len);
        assert_eq!(write(&mut page[..len], &devices[..count]).is_ok(), fits);
        if fits {
            let roster = Roster::read(&page[..len]).expect("a written page reads");
            assert_eq!(roster.len(), count);
        }
    }

    /// Falsification: replayable `falsifications/verification.capacity_of_fits_the_page_for_every_length.patch`
    /// **`capacity_of` never claims more room than the page has**: for every page length that holds
    /// the header, the header and `capacity_of(len)` entries fit in `len` bytes, and what is left
    /// over is too short for one entry more. This is the arithmetic both `write` and `read` bound their indexing by, checked at
    /// every length rather than up to `PAGE`.
    #[kani::proof]
    fn capacity_of_fits_the_page_for_every_length() {
        let len: usize = kani::any();
        kani::assume(len >= HEADER_BYTES);
        let capacity = capacity_of(len);
        assert!(HEADER_BYTES + capacity * ENTRY_BYTES <= len);
        assert!(len - HEADER_BYTES - capacity * ENTRY_BYTES < ENTRY_BYTES);
    }

    /// Falsification: replayable `falsifications/verification.read_is_total_on_a_whole_page_with_any_header.patch`
    /// **`read` is total on a whole page, whatever its header says**: on a page of 4096 bytes with
    /// any header bytes, `read` neither panics nor indexes out of bounds, and when it accepts the
    /// page, any counted entry can be fetched, the one after the last cannot, and `devices()`
    /// yields `len()` devices. The entries are zeros, since their contents decide no index.
    #[kani::proof]
    #[kani::unwind(9)]
    fn read_is_total_on_a_whole_page_with_any_header() {
        let mut page = [0u8; 4096];
        let header: [u8; HEADER_BYTES] = kani::any();
        page[..HEADER_BYTES].copy_from_slice(&header);

        if let Ok(roster) = Roster::read(&page) {
            assert!(roster.len() <= capacity_of(page.len()));
            assert_eq!(roster.devices().len(), roster.len());
            let index: usize = kani::any();
            kani::assume(index < roster.len());
            assert!(roster.get(index).is_some(), "a counted entry is missing");
            assert!(roster.get(roster.len()).is_none());
        }
    }
}
