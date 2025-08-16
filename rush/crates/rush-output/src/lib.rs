//! Rush Output - Terminal output and logging
//! 
//! This crate provides a simplified output system for Rush with a clean Sink abstraction.

pub mod cli;
pub mod simple;

// Keep minimal exports for backward compatibility
pub mod event;
pub mod formatter;
pub mod sink;
pub mod source;
pub mod stream;

// Legacy modules kept for compilation but deprecated
pub mod buffered;
pub mod config;
pub mod director;
pub mod example;
pub mod factory;
pub mod file;
pub mod filter;
pub mod router;
pub mod session;
pub mod shared;

// Re-export commonly used types
pub use source::OutputSource;
pub use stream::{OutputStream, OutputStreamType};

// Re-export legacy types for backward compatibility (deprecated)
pub use buffered::BufferedOutputDirector;
pub use director::{OutputDirector, StdOutputDirector};
pub use factory::OutputDirectorFactory;
pub use file::FileOutputDirector;
pub use shared::SharedOutputDirector;