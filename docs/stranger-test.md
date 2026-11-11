# The stranger cadence

The release's whole claim is that a stranger, given this repository and nothing else, can build
the crate and run its proofs from the README alone. A real agent runs that check on a
cadence, and the weekly `stranger cadence` job fails when the
last run is past 28 days.

What a run is:

1. Clone the repository fresh.
2. Follow the README and nothing else: build, test, and run the proofs.
3. Note every place the README lied, omitted, or assumed. Those are findings against the README,
   not against the stranger.
4. Record the date on the line below.

Last run: 2026-11-11

Result of that run: the stranger reached a passing build, 10 unit tests and 2 documentation tests,
the library on all three bare-metal targets, and all three Kani proofs in about a minute, from the
README alone. Rust 1.88, the stated minimum, passes the tests too. Kani, the toolchains and the
targets were already installed, so the run could not show whether the install steps work. It found
four gaps, none of which stopped it: the bare-metal line never says to add the targets, a passing
`cargo kani` prints warnings the README does not call expected, the README never mentions the
falsification patches, and it says to run the proofs a section before it says how to install Kani. The full record is in [the run appendix](stranger-test-run-2026-11-11.md).
