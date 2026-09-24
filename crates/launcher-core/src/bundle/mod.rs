//! Bundle module — manifest parsing and tamper-proof self-healing.

pub mod heal;
pub mod manifest;

pub use heal::*;
pub use manifest::*;
