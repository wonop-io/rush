//! Rush Output - Terminal output and logging

pub mod director;
pub mod interactive;
pub mod json;
pub mod plain;
pub mod stream;

pub use director::{OutputDirector, OutputDirectorConfig, OutputDirectorFactory};
pub use stream::{OutputStream, OutputSource};

#[cfg(feature = "interactive")]
pub use interactive::InteractiveOutputDirector;

pub type SharedOutputDirector = std::sync::Arc<Box<dyn OutputDirector>>;
