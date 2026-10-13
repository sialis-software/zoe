//! The user's vault: markdown files on disk (the source of truth), plus a SQLite index of
//! what links where (derived, disposable; docs/05 §6).

mod files;
mod index;
mod seed;

pub use files::{Vault, data_home};
pub use index::{Hit, Index, Stats};
pub use seed::{SeedMark, seed_vault, seed_notes};
