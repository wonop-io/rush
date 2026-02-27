//! Toolchain management module
//!
//! This module provides tools for detecting and managing build toolchains across
//! different platforms and architectures.

mod context;
mod platform;

pub use context::ToolchainContext;
pub use platform::{ArchType, OperatingSystem, Platform};
