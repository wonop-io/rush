//! Event system for container lifecycle management
//!
//! This module provides an event-driven architecture for decoupled
//! communication between components.

pub mod bus;
pub mod types;

// Re-export main types
pub use bus::{EventBus, EventHandler, FilteredHandler, TypedHandler, Subscription};
pub use types::{
    ContainerEvent, Event, EventLevel, EventMetadata, 
    RebuildReason, ShutdownReason, StopReason
};