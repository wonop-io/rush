//! Event bus implementation for decoupled communication
//!
//! The event bus allows components to publish events and subscribe to specific event types
//! without direct dependencies between components.

use super::types::{ContainerEvent, Event};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};

/// Trait for event handlers
#[async_trait]
pub trait EventHandler: Send + Sync {
    /// Handle an event
    async fn handle(&self, event: Event) -> Result<(), Box<dyn std::error::Error>>;
    
    /// Return true if this handler should receive the given event
    fn should_handle(&self, _event: &Event) -> bool {
        true // By default, handle all events
    }
}

/// Event subscription handle
pub struct Subscription {
    id: String,
    _drop_guard: mpsc::Sender<String>,
}

/// Event bus for publishing and subscribing to events
#[derive(Clone)]
pub struct EventBus {
    subscribers: Arc<RwLock<HashMap<String, Arc<dyn EventHandler>>>>,
    event_tx: mpsc::Sender<Event>,
    _event_task: Arc<tokio::task::JoinHandle<()>>,
}

impl EventBus {
    /// Create a new event bus
    pub fn new() -> Self {
        let (event_tx, mut event_rx) = mpsc::channel::<Event>(1000);
        let subscribers: Arc<RwLock<HashMap<String, Arc<dyn EventHandler>>>> = Arc::new(RwLock::new(HashMap::new()));
        let subscribers_clone = subscribers.clone();
        
        // Spawn task to process events
        let event_task = tokio::spawn(async move {
            while let Some(event) = event_rx.recv().await {
                let handlers = subscribers_clone.read().await;
                for (_, handler) in handlers.iter() {
                    if handler.should_handle(&event) {
                        // Clone handler and event for async processing
                        let handler = handler.clone();
                        let event = event.clone();
                        
                        // Handle events in parallel
                        tokio::spawn(async move {
                            if let Err(e) = handler.handle(event).await {
                                log::error!("Event handler error: {}", e);
                            }
                        });
                    }
                }
            }
        });

        Self {
            subscribers,
            event_tx,
            _event_task: Arc::new(event_task),
        }
    }

    /// Publish an event to all subscribers
    pub async fn publish(&self, event: Event) -> Result<(), Box<dyn std::error::Error>> {
        self.event_tx.send(event).await
            .map_err(|e| format!("Failed to publish event: {}", e))?;
        Ok(())
    }

    /// Subscribe to events with a handler
    pub async fn subscribe(
        &self,
        handler: Arc<dyn EventHandler>,
    ) -> Result<Subscription, Box<dyn std::error::Error>> {
        let id = uuid::Uuid::new_v4().to_string();
        let mut subscribers = self.subscribers.write().await;
        subscribers.insert(id.clone(), handler);
        
        // Create a drop guard to automatically unsubscribe
        let (tx, _rx) = mpsc::channel(1);
        
        Ok(Subscription {
            id,
            _drop_guard: tx,
        })
    }

    /// Unsubscribe from events
    pub async fn unsubscribe(&self, subscription: Subscription) {
        let mut subscribers = self.subscribers.write().await;
        subscribers.remove(&subscription.id);
    }

    /// Get the number of active subscribers
    pub async fn subscriber_count(&self) -> usize {
        self.subscribers.read().await.len()
    }
}

/// Builder for creating filtered event handlers
pub struct FilteredHandler<F>
where
    F: Fn(&Event) -> bool + Send + Sync,
{
    filter: F,
    handler: Arc<dyn EventHandler>,
}

impl<F> FilteredHandler<F>
where
    F: Fn(&Event) -> bool + Send + Sync,
{
    /// Create a new filtered handler
    pub fn new(filter: F, handler: Arc<dyn EventHandler>) -> Self {
        Self { filter, handler }
    }
}

#[async_trait]
impl<F> EventHandler for FilteredHandler<F>
where
    F: Fn(&Event) -> bool + Send + Sync,
{
    async fn handle(&self, event: Event) -> Result<(), Box<dyn std::error::Error>> {
        self.handler.handle(event).await
    }

    fn should_handle(&self, event: &Event) -> bool {
        (self.filter)(event)
    }
}

/// Helper to create handlers for specific event types
pub struct TypedHandler<F>
where
    F: Fn(ContainerEvent) -> Result<(), Box<dyn std::error::Error>> + Send + Sync,
{
    handler_fn: F,
}

impl<F> TypedHandler<F>
where
    F: Fn(ContainerEvent) -> Result<(), Box<dyn std::error::Error>> + Send + Sync,
{
    pub fn new(handler_fn: F) -> Self {
        Self { handler_fn }
    }
}

#[async_trait]
impl<F> EventHandler for TypedHandler<F>
where
    F: Fn(ContainerEvent) -> Result<(), Box<dyn std::error::Error>> + Send + Sync,
{
    async fn handle(&self, event: Event) -> Result<(), Box<dyn std::error::Error>> {
        (self.handler_fn)(event.payload)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Instant;

    struct TestHandler {
        counter: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl EventHandler for TestHandler {
        async fn handle(&self, _event: Event) -> Result<(), Box<dyn std::error::Error>> {
            self.counter.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_event_bus_publish_subscribe() {
        let bus = EventBus::new();
        let counter = Arc::new(AtomicUsize::new(0));
        
        let handler = Arc::new(TestHandler {
            counter: counter.clone(),
        });
        
        let _subscription = bus.subscribe(handler).await.unwrap();
        
        // Publish an event
        let event = Event::new(
            "test",
            ContainerEvent::BuildStarted {
                component: "test".to_string(),
                timestamp: Instant::now(),
            },
        );
        
        bus.publish(event).await.unwrap();
        
        // Give the handler time to process
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_multiple_subscribers() {
        let bus = EventBus::new();
        let counter1 = Arc::new(AtomicUsize::new(0));
        let counter2 = Arc::new(AtomicUsize::new(0));
        
        let handler1 = Arc::new(TestHandler {
            counter: counter1.clone(),
        });
        let handler2 = Arc::new(TestHandler {
            counter: counter2.clone(),
        });
        
        let _sub1 = bus.subscribe(handler1).await.unwrap();
        let _sub2 = bus.subscribe(handler2).await.unwrap();
        
        assert_eq!(bus.subscriber_count().await, 2);
        
        let event = Event::new(
            "test",
            ContainerEvent::NetworkReady {
                network_name: "test-network".to_string(),
            },
        );
        
        bus.publish(event).await.unwrap();
        
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        
        assert_eq!(counter1.load(Ordering::SeqCst), 1);
        assert_eq!(counter2.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_unsubscribe() {
        let bus = EventBus::new();
        let counter = Arc::new(AtomicUsize::new(0));
        
        let handler = Arc::new(TestHandler {
            counter: counter.clone(),
        });
        
        let subscription = bus.subscribe(handler).await.unwrap();
        assert_eq!(bus.subscriber_count().await, 1);
        
        bus.unsubscribe(subscription).await;
        assert_eq!(bus.subscriber_count().await, 0);
        
        // Publish event after unsubscribe
        let event = Event::new(
            "test",
            ContainerEvent::ShutdownInitiated {
                reason: crate::events::ShutdownReason::UserRequested,
            },
        );
        
        bus.publish(event).await.unwrap();
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        
        // Counter should not increment
        assert_eq!(counter.load(Ordering::SeqCst), 0);
    }
}