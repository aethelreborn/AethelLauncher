//! Modpack support — search, import, and install modpacks from CurseForge, Modrinth, and local folders.

pub mod curseforge;
pub mod install;
pub mod local;
pub mod modrinth;
pub mod types;

pub use install::{
    install_local_modpack, install_modpack, InstallParams, InstallProgress, InstallResult,
};
pub use types::*;
