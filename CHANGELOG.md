# Changelog

Dates are UTC. One line per release, what changed and why, so a reader of the diff has the
sentence beside it.

## Unreleased

- `Roster::read` returns `Result<Roster, ReadError>` instead of an `Option`, so a reader can tell a
  short mapping, a page of another kind and a broken writer apart.
- `Roster::read` checks the capacity word: it refuses a count past it, and a capacity word past the
  page.
- `ReadError` and `TooManyDevices` implement `Display` and `core::error::Error`.
- `Device` derives `Hash`, `PartialOrd` and `Ord`, sorting by transport and then by ordinal; the
  struct declares its fields in that order. The page layout stays as it was.
- `Roster::devices` returns an `ExactSizeIterator`. The parameter of `Roster::get` is `index`.
- The three transport constants carry search aliases: `virtio-mmio`, `virtio-pci` and `nvme`.
- The crate forbids `unsafe` code.
- The documentation says what a reader may trust: the header checks, and nothing about the
  ordinal or the transport of an entry.
- Two more proofs: `capacity_of` at every page length, and `read` on a whole 4096-byte page with
  any header.
- The package carries the crate, its falsification patches, the README, this file and the
  licenses, and none of the repository's workflows or settings.

## 0.1.0-pre.1

- First version. The page's identity is `INTERFACE_ID` 5, `VERSION` 1 and the magic `blkrosv1`,
  whose digits are the version.

## BUGS

- The template's lag check guards the CI pin, not this file; a change that matters to a user of
  the crate belongs in a release line here.
