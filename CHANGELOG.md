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

## 0.1.0-pre.1

- First version.

## BUGS

- The template's lag check guards the CI pin, not this file; a change that matters to a user of
  the crate belongs in a release line here.
