//! State machine for container lifecycle management
//!
//! This module provides a type-safe state machine for managing container
//! lifecycle transitions and ensuring valid state changes.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Container lifecycle states
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ContainerState {
    /// Container doesn't exist
    NotCreated,
    /// Container is being created
    Creating,
    /// Container exists but is stopped
    Created,
    /// Container is starting
    Starting,
    /// Container is running
    Running,
    /// Container is being paused
    Pausing,
    /// Container is paused
    Paused,
    /// Container is being unpaused
    Resuming,
    /// Container is stopping
    Stopping,
    /// Container is stopped
    Stopped,
    /// Container is being removed
    Removing,
    /// Container has been removed
    Removed,
    /// Container is in error state
    Error,
}

impl fmt::Display for ContainerState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotCreated => write!(f, "not_created"),
            Self::Creating => write!(f, "creating"),
            Self::Created => write!(f, "created"),
            Self::Starting => write!(f, "starting"),
            Self::Running => write!(f, "running"),
            Self::Pausing => write!(f, "pausing"),
            Self::Paused => write!(f, "paused"),
            Self::Resuming => write!(f, "resuming"),
            Self::Stopping => write!(f, "stopping"),
            Self::Stopped => write!(f, "stopped"),
            Self::Removing => write!(f, "removing"),
            Self::Removed => write!(f, "removed"),
            Self::Error => write!(f, "error"),
        }
    }
}

/// Container lifecycle events that trigger state transitions
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContainerEvent {
    Create,
    Created,
    Start,
    Started,
    Pause,
    Paused,
    Resume,
    Resumed,
    Stop,
    Stopped,
    Remove,
    Removed,
    Failed,
    Recover,
}

/// State transition rule
#[derive(Debug, Clone)]
pub struct StateTransition {
    pub from: ContainerState,
    pub event: ContainerEvent,
    pub to: ContainerState,
}

/// Container state machine
pub struct ContainerStateMachine {
    current_state: ContainerState,
    transitions: Vec<StateTransition>,
}

impl ContainerStateMachine {
    /// Create a new state machine starting from NotCreated
    pub fn new() -> Self {
        Self::with_initial_state(ContainerState::NotCreated)
    }
    
    /// Create a new state machine with a specific initial state
    pub fn with_initial_state(initial: ContainerState) -> Self {
        let transitions = vec![
            // Creation transitions
            StateTransition {
                from: ContainerState::NotCreated,
                event: ContainerEvent::Create,
                to: ContainerState::Creating,
            },
            StateTransition {
                from: ContainerState::Creating,
                event: ContainerEvent::Created,
                to: ContainerState::Created,
            },
            
            // Start transitions
            StateTransition {
                from: ContainerState::Created,
                event: ContainerEvent::Start,
                to: ContainerState::Starting,
            },
            StateTransition {
                from: ContainerState::Starting,
                event: ContainerEvent::Started,
                to: ContainerState::Running,
            },
            StateTransition {
                from: ContainerState::Stopped,
                event: ContainerEvent::Start,
                to: ContainerState::Starting,
            },
            
            // Pause transitions
            StateTransition {
                from: ContainerState::Running,
                event: ContainerEvent::Pause,
                to: ContainerState::Pausing,
            },
            StateTransition {
                from: ContainerState::Pausing,
                event: ContainerEvent::Paused,
                to: ContainerState::Paused,
            },
            
            // Resume transitions
            StateTransition {
                from: ContainerState::Paused,
                event: ContainerEvent::Resume,
                to: ContainerState::Resuming,
            },
            StateTransition {
                from: ContainerState::Resuming,
                event: ContainerEvent::Resumed,
                to: ContainerState::Running,
            },
            
            // Stop transitions
            StateTransition {
                from: ContainerState::Running,
                event: ContainerEvent::Stop,
                to: ContainerState::Stopping,
            },
            StateTransition {
                from: ContainerState::Paused,
                event: ContainerEvent::Stop,
                to: ContainerState::Stopping,
            },
            StateTransition {
                from: ContainerState::Stopping,
                event: ContainerEvent::Stopped,
                to: ContainerState::Stopped,
            },
            
            // Remove transitions
            StateTransition {
                from: ContainerState::Created,
                event: ContainerEvent::Remove,
                to: ContainerState::Removing,
            },
            StateTransition {
                from: ContainerState::Stopped,
                event: ContainerEvent::Remove,
                to: ContainerState::Removing,
            },
            StateTransition {
                from: ContainerState::Removing,
                event: ContainerEvent::Removed,
                to: ContainerState::Removed,
            },
            
            // Error transitions - any state can fail
            StateTransition {
                from: ContainerState::Creating,
                event: ContainerEvent::Failed,
                to: ContainerState::Error,
            },
            StateTransition {
                from: ContainerState::Starting,
                event: ContainerEvent::Failed,
                to: ContainerState::Error,
            },
            StateTransition {
                from: ContainerState::Running,
                event: ContainerEvent::Failed,
                to: ContainerState::Error,
            },
            StateTransition {
                from: ContainerState::Pausing,
                event: ContainerEvent::Failed,
                to: ContainerState::Error,
            },
            StateTransition {
                from: ContainerState::Resuming,
                event: ContainerEvent::Failed,
                to: ContainerState::Error,
            },
            StateTransition {
                from: ContainerState::Stopping,
                event: ContainerEvent::Failed,
                to: ContainerState::Error,
            },
            StateTransition {
                from: ContainerState::Removing,
                event: ContainerEvent::Failed,
                to: ContainerState::Error,
            },
            
            // Recovery from error
            StateTransition {
                from: ContainerState::Error,
                event: ContainerEvent::Recover,
                to: ContainerState::Stopped,
            },
        ];
        
        Self {
            current_state: initial,
            transitions,
        }
    }
    
    /// Get the current state
    pub fn current_state(&self) -> ContainerState {
        self.current_state
    }
    
    /// Check if a transition is valid
    pub fn can_transition(&self, event: ContainerEvent) -> bool {
        self.find_transition(event).is_some()
    }
    
    /// Apply an event and transition to the next state
    pub fn transition(&mut self, event: ContainerEvent) -> Result<ContainerState, StateError> {
        if let Some(transition) = self.find_transition(event) {
            let old_state = self.current_state;
            self.current_state = transition.to;
            log::debug!(
                "Container state transition: {} -> {} (event: {:?})",
                old_state, self.current_state, event
            );
            Ok(self.current_state)
        } else {
            Err(StateError::InvalidTransition {
                from: self.current_state,
                event,
            })
        }
    }
    
    /// Find a valid transition for the given event
    fn find_transition(&self, event: ContainerEvent) -> Option<&StateTransition> {
        self.transitions
            .iter()
            .find(|t| t.from == self.current_state && t.event == event)
    }
    
    /// Get all valid events from the current state
    pub fn valid_events(&self) -> Vec<ContainerEvent> {
        self.transitions
            .iter()
            .filter(|t| t.from == self.current_state)
            .map(|t| t.event)
            .collect()
    }
    
    /// Check if the container is in a running state
    pub fn is_running(&self) -> bool {
        self.current_state == ContainerState::Running
    }
    
    /// Check if the container is in a terminal state
    pub fn is_terminal(&self) -> bool {
        matches!(self.current_state, ContainerState::Removed | ContainerState::Error)
    }
    
    /// Check if the container can be started
    pub fn can_start(&self) -> bool {
        matches!(
            self.current_state,
            ContainerState::Created | ContainerState::Stopped
        )
    }
    
    /// Check if the container can be stopped
    pub fn can_stop(&self) -> bool {
        matches!(
            self.current_state,
            ContainerState::Running | ContainerState::Paused
        )
    }
}

impl Default for ContainerStateMachine {
    fn default() -> Self {
        Self::new()
    }
}

/// State machine errors
#[derive(Debug, thiserror::Error)]
pub enum StateError {
    #[error("Invalid state transition from {from} with event {event:?}")]
    InvalidTransition {
        from: ContainerState,
        event: ContainerEvent,
    },
}

/// Service lifecycle states (higher level than container states)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ServiceState {
    /// Service is not deployed
    Undeployed,
    /// Service is being deployed
    Deploying,
    /// Service is deployed and healthy
    Healthy,
    /// Service is deployed but unhealthy
    Unhealthy,
    /// Service is being updated
    Updating,
    /// Service is being scaled
    Scaling,
    /// Service is being removed
    Removing,
    /// Service has been removed
    Removed,
    /// Service is in maintenance mode
    Maintenance,
}

impl fmt::Display for ServiceState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Undeployed => write!(f, "undeployed"),
            Self::Deploying => write!(f, "deploying"),
            Self::Healthy => write!(f, "healthy"),
            Self::Unhealthy => write!(f, "unhealthy"),
            Self::Updating => write!(f, "updating"),
            Self::Scaling => write!(f, "scaling"),
            Self::Removing => write!(f, "removing"),
            Self::Removed => write!(f, "removed"),
            Self::Maintenance => write!(f, "maintenance"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_basic_lifecycle() {
        let mut sm = ContainerStateMachine::new();
        assert_eq!(sm.current_state(), ContainerState::NotCreated);
        
        // Create container
        assert!(sm.transition(ContainerEvent::Create).is_ok());
        assert_eq!(sm.current_state(), ContainerState::Creating);
        
        assert!(sm.transition(ContainerEvent::Created).is_ok());
        assert_eq!(sm.current_state(), ContainerState::Created);
        
        // Start container
        assert!(sm.transition(ContainerEvent::Start).is_ok());
        assert_eq!(sm.current_state(), ContainerState::Starting);
        
        assert!(sm.transition(ContainerEvent::Started).is_ok());
        assert_eq!(sm.current_state(), ContainerState::Running);
        assert!(sm.is_running());
        
        // Stop container
        assert!(sm.transition(ContainerEvent::Stop).is_ok());
        assert_eq!(sm.current_state(), ContainerState::Stopping);
        
        assert!(sm.transition(ContainerEvent::Stopped).is_ok());
        assert_eq!(sm.current_state(), ContainerState::Stopped);
        
        // Remove container
        assert!(sm.transition(ContainerEvent::Remove).is_ok());
        assert_eq!(sm.current_state(), ContainerState::Removing);
        
        assert!(sm.transition(ContainerEvent::Removed).is_ok());
        assert_eq!(sm.current_state(), ContainerState::Removed);
        assert!(sm.is_terminal());
    }
    
    #[test]
    fn test_invalid_transition() {
        let mut sm = ContainerStateMachine::new();
        
        // Can't start from NotCreated
        assert!(sm.transition(ContainerEvent::Start).is_err());
        
        // Can't remove from NotCreated
        assert!(sm.transition(ContainerEvent::Remove).is_err());
    }
    
    #[test]
    fn test_pause_resume() {
        let mut sm = ContainerStateMachine::with_initial_state(ContainerState::Running);
        
        // Pause
        assert!(sm.transition(ContainerEvent::Pause).is_ok());
        assert_eq!(sm.current_state(), ContainerState::Pausing);
        
        assert!(sm.transition(ContainerEvent::Paused).is_ok());
        assert_eq!(sm.current_state(), ContainerState::Paused);
        
        // Resume
        assert!(sm.transition(ContainerEvent::Resume).is_ok());
        assert_eq!(sm.current_state(), ContainerState::Resuming);
        
        assert!(sm.transition(ContainerEvent::Resumed).is_ok());
        assert_eq!(sm.current_state(), ContainerState::Running);
    }
    
    #[test]
    fn test_error_handling() {
        let mut sm = ContainerStateMachine::with_initial_state(ContainerState::Starting);
        
        // Transition to error
        assert!(sm.transition(ContainerEvent::Failed).is_ok());
        assert_eq!(sm.current_state(), ContainerState::Error);
        assert!(sm.is_terminal());
        
        // Recover from error
        assert!(sm.transition(ContainerEvent::Recover).is_ok());
        assert_eq!(sm.current_state(), ContainerState::Stopped);
    }
    
    #[test]
    fn test_valid_events() {
        let sm = ContainerStateMachine::with_initial_state(ContainerState::Running);
        let events = sm.valid_events();
        
        assert!(events.contains(&ContainerEvent::Stop));
        assert!(events.contains(&ContainerEvent::Pause));
        assert!(events.contains(&ContainerEvent::Failed));
        assert!(!events.contains(&ContainerEvent::Start));
    }
}