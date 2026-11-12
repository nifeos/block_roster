# Fuzz seeds

Inputs the two fuzz targets start from, committed on purpose. Pass a directory here as a read-only
corpus after the target's own scratch corpus:

```sh
cargo +nightly fuzz run block_roster_read fuzz/corpus/block_roster_read fuzz/seeds/block_roster_read
cargo +nightly fuzz run block_roster_write fuzz/corpus/block_roster_write fuzz/seeds/block_roster_write
```

| Target | Seeds | What they are |
|---|---|---|
| `block_roster_read` | `block_roster_read/` | a 48-byte page with three devices (one per transport), and an empty 16-byte page |
| `block_roster_write` | `block_roster_write/` | a 48-byte page length followed by three devices that fit, and a 24-byte page length followed by two devices that do not |
