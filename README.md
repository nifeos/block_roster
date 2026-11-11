# block_roster_contract

[![check](https://github.com/nifeos/block_roster_contract/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/nifeos/block_roster_contract/actions/workflows/ci.yml?query=branch%3Amain)
[![proofs](https://github.com/nifeos/block_roster_contract/actions/workflows/proofs.yml/badge.svg?branch=main)](https://github.com/nifeos/block_roster_contract/actions/workflows/proofs.yml?query=branch%3Amain)
[![license](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](https://github.com/nifeos/block_roster_contract#license)
[![minimum Rust version](https://img.shields.io/badge/rust-1.88%2B-blue.svg)](https://github.com/nifeos/block_roster_contract/blob/main/Cargo.toml)

The page that answers "what block devices are attached": a small, fixed-width, pointer-free layout
that one party writes once, at wiring time, and that other programs then map read-only and read.
`no_std`, no dependencies. The writer and the readers share this one definition, so they cannot
drift.

This is a shared page, not a message protocol. There is no request, no reply and no server. Holding
a roster tells a program that a device exists; it does not let the program touch it. Something else decides which program
gets which device's endpoint, so a program without the page has nowhere to
look.

## The contract

The page is little-endian:

| bytes | field |
|---|---|
| `0..8` | `MAGIC`, the ASCII text `blkrosv1` read as a little-endian `u64` |
| `8..12` | count: how many entries follow |
| `12..16` | capacity: how many entries this page has room for |
| `16..` | entries, `ENTRY_BYTES` (8) each |

An entry is a `u32` ordinal (which device of this transport, counting from 0) and a `u32` transport,
one of `TRANSPORT_VIRTIO_MEMORY_MAPPED_IO` (0), `TRANSPORT_VIRTIO_PCI` (1) and
`TRANSPORT_NON_VOLATILE_MEMORY_EXPRESS` (2).

- The roster carries no capacity in bytes. Ask the device for its size.
- `write` refuses with `TooManyDevices`, and leaves the page untouched, when the page cannot hold
  the header and the devices; a page shorter than the header holds no roster at all.
- `Roster::read` refuses a page with a `ReadError` that names the refusal. It is `TooShort` when
  the page is shorter than the header, and `NotARoster` when the magic is wrong (a zeroed frame is
  not an empty roster). It is `CountBeyondPage` when the count runs past what the page holds,
  `CapacityBeyondPage` when the capacity word does, and `CountBeyondCapacity` when the count is
  larger than the capacity word. A page longer than its capacity word reads, since a reader may map
  more than the writer laid out. A reader never indexes past its own mapping.
- `Device` sorts by transport and then by ordinal, so a sorted listing groups the devices by bus.
  `Roster::devices` knows its length.
- A number the reader does not know as a transport has no name (`Device::transport_name` returns
  `None`); the reader decides what to do with it.
- The identity never changes: `INTERFACE_ID` 5, `VERSION` 1 and the magic `blkrosv1`.
  `INTERFACE_ID` names this interface when a program declares which interfaces it links, and it
  stays 5 for good. The page has one version, which is also the digits of the magic. A layout
  break bumps both in one change, never the id, and an old reader then refuses the page.

## EXAMPLES

```rust
use block_roster_contract::{Device, Roster, TRANSPORT_VIRTIO_PCI};

// The writer lays the roster into a page it owns.
let mut page = [0u8; 4096];
block_roster_contract::write(&mut page, &[Device { ordinal: 0, transport: TRANSPORT_VIRTIO_PCI }]).unwrap();

// A reader holds the mapped page and borrows from it.
let roster = Roster::read(&page).unwrap();
assert_eq!(roster.len(), 1);
assert_eq!(roster.get(0).unwrap().transport_name(), Some("virtio-pci"));
assert_eq!(roster.count_by_transport(TRANSPORT_VIRTIO_PCI), 1);
```

## What is proven

Three Kani harnesses in `src/lib.rs` prove three things. `Roster::read` is total on any bytes.
`write` is total on any page length and refuses a page too short for its header. `write` then
`read` returns the same devices in order. Install Kani and run them as "Building and testing" below describes.

## Building and testing

```sh
cargo build
cargo test
rustup target add x86_64-unknown-none aarch64-unknown-none-softfloat riscv64imac-unknown-none-elf
cargo build --lib --target x86_64-unknown-none   # also aarch64-unknown-none-softfloat, riscv64imac-unknown-none-elf
```

Run `rustup target add` once, before the first bare-metal build. Without it, that build stops with
`can't find crate for core`. The tests run on the host and cover 10 unit tests and 2 documentation
tests, one of which is the example above.

### Running the proofs

The proofs need Kani, which `rustup` does not install. Run `cargo install --locked
kani-verifier --version 0.68.0`, then `cargo kani setup`. The setup step downloads a model
checker, so it needs network access. CI uses version 0.68.0, and another version may verify
differently.

Then `cargo kani`, from the crate's directory, runs all three harnesses. On an x86_64 Linux desktop
that takes about a minute and peaks near 1.5 gigabytes of memory. CI runs plain `cargo kani`, with
no extra flags. To run one harness, give its full path and `--exact`:

```sh
cargo kani --harness verification::write_is_total_on_any_page_length --exact
```

A passing run prints several lines that look alarming. All of them are expected:

- `warning: use of an unstable feature`, about `#![feature(register_tool)]`, which Kani sets.
- `warning: Found the following unsupported constructs: caller_location (1), foreign function (1)`,
  followed by "Verification will fail if one or more of these constructs is reachable". A harness
  that ends in `VERIFICATION:- SUCCESSFUL` never reached them.
- Many `aborting path on assume(false)` lines. They mark paths that a harness's assumptions
  rule out.
- Checks described as `internal error: entered unreachable code` or unreachable code, some of them
  under `Status: SUCCESS`. They report code that cannot run, and they are no failure.
- Source paths under `/home/runner/.rustup`, a directory that does not exist on your machine. They
  come from the toolchain Kani ships.

Trust the last line of the run: `Complete - 3 successfully verified harnesses, 0 failures, 3 total.`

### Running one falsification patch

```sh
patch -p1 --no-backup-if-mismatch < falsifications/verification.write_is_total_on_any_page_length.patch
cargo kani --harness verification::write_is_total_on_any_page_length --exact   # ends VERIFICATION:- FAILED
git checkout -- src
```

The harness name is the part of the patch's file name after `verification.`. The run ends with
`Complete - 0 successfully verified harnesses, 1 failures, 1 total.`, and the failed check names
the assertion that the patch breaks. `git checkout -- src` reverts the patch.

## BUGS

- The harnesses check a page of up to three entries (40 bytes). A longer page is not
  machine-checked.

## Minimum Rust version

Rust 1.88. To check it, install that toolchain and a bare-metal target, then test and build with it:

```sh
rustup toolchain install 1.88 --profile minimal --target x86_64-unknown-none
cargo +1.88 test
cargo +1.88 build --lib --target x86_64-unknown-none
```

## License

Licensed under either of MIT (LICENSE-MIT) or Apache-2.0 (LICENSE-APACHE), at your option.
