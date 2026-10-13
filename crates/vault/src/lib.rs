//! The user's vault: markdown files on disk (the source of truth), plus a SQLite index of
//! what links where (derived, disposable; docs/05 §6).

mod bible_index;
mod files;
mod index;
mod seed;

pub use bible_index::{BibleIndex, Search, VerseHit};
pub use files::{OLD_DATA_FOLDER, Vault, data_home, migrate_data_home};
pub use index::{Hit, Index, Stats};
pub use seed::{SeedMark, seed_vault, seed_notes};
