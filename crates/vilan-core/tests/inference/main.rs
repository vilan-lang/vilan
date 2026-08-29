//! Compile-outcome tests for the type inference / generic resolution paths that
//! have been the source of recurring bugs. Each case asserts whether a source
//! compiles cleanly or fails, run through the real pipeline on a large-stack
//! worker (so a recursion bug surfaces as an error, not an aborted suite).
//!
//! `#[ignore]`d tests are KNOWN BUGS (see proposal/analyzer-refactor.md):
//! they assert the *desired* outcome, so removing `#[ignore]` when the bug is
//! fixed turns them green — that's how we track progress against the plan.
//!
//! B145 split the old single `tests/inference.rs` (69k lines, the file every
//! lane appended to and every merge fought over) into the subject modules
//! below. They are `mod`s of ONE test binary on purpose: each top-level file
//! under `tests/` links the whole crate (`suite-speed.md` E21), so one file per
//! subject would have bought N-1 extra link steps. `cargo test -p vilan-core
//! --test inference` is unchanged; a new pin goes in the subject module that
//! owns its area, and a new subject is a new module here.

mod support;

mod backed_enums;
mod borrows;
mod bounds;
mod generics;
mod hmr;
mod iterators;
mod liveness;
mod macros;
mod markdown;
mod modules;
mod platform;
mod resources;
mod returns;
mod std_surface;
mod styling;
mod tuples;
