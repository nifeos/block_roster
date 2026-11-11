# Stranger run, 2026-11-11

The first run of this repository's stranger test, against `main` at `47ac5a9`, from 03:11 to 03:20
UTC. This page records what the stranger did, what the README did not cover, and what this run
could not measure.

## How it was run

- A subagent on Claude Opus 5.5, told to act as a stranger trying the crate for the first time.
- It cloned `main` from GitHub into a new empty directory and followed `README.md`. Beyond the
  README it read `CONTRIBUTING.md`, this page, `.gitignore`, the commit hook, the pull request
  template and the list of files under `falsifications/` and `.github/`. It did not read
  `src/lib.rs` beyond a search for the harnesses.
- It was not a clean stranger. Its context held the maintainers' workspace instructions and memory
  from turn zero, so it knew this organization's conventions before it opened the README. Treat
  every "the README was enough" below as weaker evidence than a finding.
- The machine had stable Rust 1.99.1, a 1.88 toolchain, the three bare-metal targets,
  and Kani 0.68.0 with its setup done.

## Result

| Measure | Outcome |
|---|---|
| Tests | `cargo test` passed: 10 unit tests, 2 documentation tests (one is the README's example) |
| Bare-metal builds | `cargo build --lib --target` passed for `x86_64-unknown-none`, `aarch64-unknown-none-softfloat` and `riscv64imac-unknown-none-elf` |
| Proofs | `cargo kani`: `Complete - 3 successfully verified harnesses, 0 failures, 3 total.` in 56 seconds |
| Minimum Rust | `cargo +1.88 test` passed, and `cargo +1.88 build --lib --target x86_64-unknown-none` |
| Falsification patch | `write_is_total_on_any_page_length`'s patch made that harness fail: `0 successfully verified harnesses, 1 failures, 1 total.` |
| Kani installed by the stranger | no, so the run did not test the install path |

## Where the README was not enough

1. **The bare-metal build line assumes the reader has the targets.** The README gives
   `cargo build --lib --target x86_64-unknown-none` and names the other two targets, but never says
   `rustup target add`. On this machine every toolchain at 1.88 or later already had them, so the
   stranger reproduced the failure on 1.85 with `--ignore-rust-version`:
   `error[E0463]: can't find crate for core`, with rustc's own hint
   `consider downloading the target with rustup target add aarch64-unknown-none-softfloat`. The
   hint names the fix, so a newcomer recovers in one step, but the README should not need it.
2. **A passing `cargo kani` prints warnings the README does not call expected.** Each harness
   prints `warning: use of an unstable feature` (`#![feature(register_tool)]`) and
   `warning: Found the following unsupported constructs: caller_location (1), foreign function (1)
   ... Verification will fail if one or more of these constructs is reachable.` Several checks
   under `Status: SUCCESS` carry the description `internal error: entered unreachable code`. A
   reader who greps the output for `error` or `warning` meets these first. The final line is
   unambiguous, and the README says nothing about the rest.
3. **The README never mentions the falsification patches.** `falsifications/` holds one patch per
   harness, and `CONTRIBUTING.md` says only that a harness must carry one. Nothing says what a patch
   is for or how to run one. The stranger guessed `git apply <patch>`, then
   `cargo kani --harness verification::<name> --exact`, then `git checkout -- src`, and the guess
   worked.
4. **The README tells the reader to run the proofs before it says how to install Kani.** "What is
   proven" ends with "Run them with `cargo kani`"; the install command comes a section later, under
   "Building and testing". A reader who stops at the first instruction on a machine without Kani
   gets an unknown-command error from cargo and no pointer forward.

No statement in the README was false. The layout table, the three proven properties, the 40-byte
bound in BUGS (a 16-byte header and three 8-byte entries) and the minimum Rust version all held.

## Limits of this run

- The machine already had Kani, its setup, the 1.88 toolchain and the targets, so nothing
  measured the install instructions.
- The stranger was a subagent with the maintainers' instructions in context, not a separate process
  with nothing loaded.
- The run recorded the gaps and changed nothing in the README. The README now covers the four
  gaps above.
