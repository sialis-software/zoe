//! The editor model behind the live-preview writer: GUI-free, so it can be tested without a
//! window. See docs/06.

pub mod blocks;
pub mod document;
pub mod render;
pub mod vim;

pub use blocks::{Block, Kind, block_at};
pub use document::{Action, Document, Motion, Selection, Wrap};
pub use vim::{Key as VimKey, Mode as VimMode, Vim};
pub use render::{Deco, Rendered, Reveal, Role, Run, Seg, SegKind, Style, is_external, render};
