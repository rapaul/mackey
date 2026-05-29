//! Shared types and the keymap engine for mackey.
//!
//! This crate is the pure, heavily-tested core: built-in keymap tables, IPC
//! message types, and the table-driven keymap engine. It has no I/O and no
//! kernel dependencies so it can be unit-tested in isolation.

/// The mackey version string, sourced from the crate version at build time.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_is_populated() {
        assert!(!VERSION.is_empty());
    }
}
