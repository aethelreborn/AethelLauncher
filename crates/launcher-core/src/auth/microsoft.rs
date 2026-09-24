//! Microsoft authentication is reserved for a later implementation.
//!
//! This explicit placeholder keeps `auth::microsoft` a valid module while
//! preventing the import from implying that a token exchange is available.

/// Whether Microsoft sign-in is enabled in this build.
pub const ENABLED: bool = false;

/// Report whether the Microsoft flow is available in this build.
pub const fn is_enabled() -> bool {
    ENABLED
}
