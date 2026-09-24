//! Backend services.
//!
//! `cache.rs` and `manifest_refresh.rs` are not wired yet (they depend on the
//! bundle-manifest pipeline) and are intentionally not declared, so the crate
//! only builds the services it actually uses.

pub mod supabase;
