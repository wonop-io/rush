//! Error context utilities for improved error handling
//!
//! This module provides traits and utilities to add context to errors,
//! replacing the repetitive map_err patterns throughout the codebase.

use crate::error::{Error, Result};
use std::fmt::Display;

/// Extension trait for adding context to Result types
pub trait ErrorContext<T> {
    /// Add a static context message to an error
    fn context(self, msg: &str) -> Result<T>;

    /// Add a dynamic context message to an error
    fn with_context<F>(self, f: F) -> Result<T>
    where
        F: FnOnce() -> String;
}

impl<T, E> ErrorContext<T> for std::result::Result<T, E>
where
    E: std::error::Error + Send + Sync + 'static,
{
    fn context(self, msg: &str) -> Result<T> {
        self.map_err(|e| Error::Internal(format!("{msg}: {e}")))
    }

    fn with_context<F>(self, f: F) -> Result<T>
    where
        F: FnOnce() -> String,
    {
        self.map_err(|e| Error::Internal(format!("{}: {}", f(), e)))
    }
}

/// Extension trait for Option types to convert to Result with context
pub trait OptionContext<T> {
    /// Convert None to an error with context
    fn context(self, msg: &str) -> Result<T>;

    /// Convert None to an error with dynamic context
    fn with_context<F>(self, f: F) -> Result<T>
    where
        F: FnOnce() -> String;
}

impl<T> OptionContext<T> for Option<T> {
    fn context(self, msg: &str) -> Result<T> {
        self.ok_or_else(|| Error::Internal(msg.to_string()))
    }

    fn with_context<F>(self, f: F) -> Result<T>
    where
        F: FnOnce() -> String,
    {
        self.ok_or_else(|| Error::Internal(f()))
    }
}

/// Helper function to format error messages consistently
pub fn format_error<T: Display>(action: &str, error: T) -> String {
    format!("Failed to {action}: {error}")
}

/// Helper macro for creating context closures
#[macro_export]
macro_rules! context {
    ($fmt:expr) => {
        || format!($fmt)
    };
    ($fmt:expr, $($arg:tt)*) => {
        || format!($fmt, $($arg)*)
    };
}
