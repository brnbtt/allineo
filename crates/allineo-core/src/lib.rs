//! Platform-independent core of allineo.
//!
//! Everything in here is plain Rust so it can be unit tested on any machine:
//! the snapping engine, the geometry types and the `#[repr(C)]` layout of the
//! shared-memory block the app and the hook DLL use to talk to each other.

pub mod config;
pub mod engine;
pub mod geom;
pub mod shared;

pub use config::Config;
pub use engine::Session;
pub use geom::{Insets, Rect};
