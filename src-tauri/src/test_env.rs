//! Test-only helpers shared across modules.
//!
//! Several modules resolve their config path straight from `$HOME` (or
//! `%APPDATA%`) with no injection seam, so their tests have to redirect the
//! process environment — which is global, while Rust runs tests on threads.
//!
//! One lock per module does not serialise those redirects against each other.
//! `auth` and `shortcut` each started out with their own private `HOME_LOCK`, and
//! running both modules in the same binary failed 8 times out of 8 as each
//! restored the developer's real `HOME` while the other was still using its temp
//! one. Anything that mutates `HOME`/`APPDATA` must therefore hold *this* lock
//! for as long as its redirect is in place.
//!
//! Tests that instead re-exec the binary as a single-threaded child with the
//! environment pinned at spawn (see `snooze`, `diagnostics`) need no lock: the
//! parent's environment is never touched, so they are immune by construction.

use std::sync::Mutex;

pub(crate) static HOME_LOCK: Mutex<()> = Mutex::new(());
