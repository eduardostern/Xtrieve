# Vendored Btrieve/N 5.10a engine

These crates are a clean-room implementation of the Btrieve/N 5.10a file
format and operation engine, copied from the local `B` project:

- `btr-format/src` from `/Users/eduardo/B/crates/btr-format/src`
- `btr-engine/src` from `/Users/eduardo/B/crates/btr-engine/src`

Snapshot date: 2026-11-07. `SHA256SUMS` records the vendored sources.

The source implements the format and behavior from documentation and
differential black-box observation. No Novell/Pervasive binary, SDK source,
or generated proprietary fixture is included here.

Xtrieve-specific changes are session-scoped transactions, session-owned
handles, per-session directory state, and disconnect cleanup. These changes
are kept in the vendored `btr-engine/src/lib.rs`; the single-session `call`
path used by the C ABI is intended to remain byte-identical.

Only the library sources are vendored. `BTRIEVE.EXE`, the B fixture tree,
the oracle, `btr-ffi`, and `btr-cli` are deliberately not copied.

License: MIT OR Apache-2.0. See `LICENSE-MIT` and `LICENSE-APACHE`.
