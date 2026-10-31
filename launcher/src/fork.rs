//! Free-Horizon's own two switches, kept apart so an upstream update never changes them.
//!
//! This file exists only in this fork: upstream has no `fork.rs`, so a merge from
//! `horizon-rust/horizon` keeps it untouched while every other launcher file updates as usual.
//! Change the values here, not the code that reads them, to keep the fork's behaviour.

/// Microsoft sign-in is **optional**: offline play (offline names, and the game's own name) is
/// on unless the player turns it off in Settings → Account. Signing in still adds online servers,
/// skins, and Mojang's own downloads; it is never required to play.
pub const OFFLINE_PLAY: bool = true;

/// Nightly builds are developer builds: release launchers list and update to published `v*`
/// game releases only, never a Nightly of Minecraft: Rust Edition. A debug build (`cargo build`)
/// still sees them.
pub const NIGHTLIES: bool = cfg!(debug_assertions);
