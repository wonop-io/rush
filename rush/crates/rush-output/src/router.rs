use crate::event::OutputEvent;
use crate::filter::OutputFilter;
use crate::sink::OutputSink;
use async_trait::async_trait;
use rush_core::error::Result;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

/// Statistics for router operations
#[derive(Debug, Clone, Default)]
pub struct RouterStats {
    pub events_routed: u64,
    pub events_dropped: u64,
    pub routing_errors: u64,
    pub total_latency_ms: u64,
}

impl RouterStats {
    /// Get average routing latency in milliseconds
    pub fn avg_latency_ms(&self) -> f64 {
        if self.events_routed == 0 {
            0.0
        } else {
            self.total_latency_ms as f64 / self.events_routed as f64
        }
    }
}

/// Routes output events to appropriate destinations
#[async_trait]
pub trait OutputRouter: Send + Sync {
    /// Route an event to its destination(s)
    async fn route(&mut self, event: OutputEvent) -> Result<()>;

    /// Get statistics about routing
    fn stats(&self) -> RouterStats;

    /// Flush all pending data
    async fn flush(&mut self) -> Result<()>;

    /// Close the router and all its sinks
    async fn close(&mut self) -> Result<()>;
}

/// Sends events to multiple destinations
pub struct BroadcastRouter {
    destinations: Vec<Box<dyn OutputSink>>,
    parallel: bool,
    stats: RouterStats,
}

impl BroadcastRouter {
    /// Create a new broadcast router
    pub fn new(destinations: Vec<Box<dyn OutputSink>>) -> Self {
        Self {
            destinations,
            parallel: true,
            stats: RouterStats::default(),
        }
    }

    /// Set whether to send to destinations in parallel
    pub fn set_parallel(mut self, parallel: bool) -> Self {
        self.parallel = parallel;
        self
    }
}

#[async_trait]
impl OutputRouter for BroadcastRouter {
    async fn route(&mut self, event: OutputEvent) -> Result<()> {
        let start = std::time::Instant::now();

        if self.parallel {
            // Send to all destinations in parallel
            // Note: We can't actually do parallel processing with mutable references
            // so we process sequentially for now

            for dest in &mut self.destinations {
                let event_clone = event.clone();
                // We need to handle this differently since we can't move the mutable reference
                // For now, we'll do sequential processing
                dest.write(event_clone).await?;
            }
        } else {
            // Send to destinations sequentially
            for dest in &mut self.destinations {
                if let Err(e) = dest.write(event.clone()).await {
                    self.stats.routing_errors += 1;
                    log::warn!("Failed to write to destination: {}", e);
                }
            }
        }

        self.stats.events_routed += 1;
        self.stats.total_latency_ms += start.elapsed().as_millis() as u64;

        Ok(())
    }

    fn stats(&self) -> RouterStats {
        self.stats.clone()
    }

    async fn flush(&mut self) -> Result<()> {
        for dest in &mut self.destinations {
            dest.flush().await?;
        }
        Ok(())
    }

    async fn close(&mut self) -> Result<()> {
        for dest in &mut self.destinations {
            dest.close().await?;
        }
        Ok(())
    }
}

/// A routing rule
pub struct RoutingRule {
    pub filter: Box<dyn OutputFilter>,
    pub sink: Box<dyn OutputSink>,
    pub stop_on_match: bool,
}

/// Routes based on rules
pub struct RuleBasedRouter {
    rules: Vec<RoutingRule>,
    default_sink: Box<dyn OutputSink>,
    stats: RouterStats,
}

impl RuleBasedRouter {
    /// Create a new rule-based router
    pub fn new(default_sink: Box<dyn OutputSink>) -> Self {
        Self {
            rules: Vec::new(),
            default_sink,
            stats: RouterStats::default(),
        }
    }

    /// Add a routing rule
    pub fn add_rule(mut self, rule: RoutingRule) -> Self {
        self.rules.push(rule);
        self
    }
}

#[async_trait]
impl OutputRouter for RuleBasedRouter {
    async fn route(&mut self, event: OutputEvent) -> Result<()> {
        let start = std::time::Instant::now();
        let mut matched = false;

        for rule in &mut self.rules {
            if rule.filter.should_pass(&event) {
                if let Err(e) = rule.sink.write(event.clone()).await {
                    self.stats.routing_errors += 1;
                    log::warn!("Failed to write to sink: {}", e);
                }
                matched = true;

                if rule.stop_on_match {
                    break;
                }
            }
        }

        // Send to default sink if no rules matched
        if !matched {
            if let Err(e) = self.default_sink.write(event).await {
                self.stats.routing_errors += 1;
                log::warn!("Failed to write to default sink: {}", e);
            }
        }

        self.stats.events_routed += 1;
        self.stats.total_latency_ms += start.elapsed().as_millis() as u64;

        Ok(())
    }

    fn stats(&self) -> RouterStats {
        self.stats.clone()
    }

    async fn flush(&mut self) -> Result<()> {
        for rule in &mut self.rules {
            rule.sink.flush().await?;
        }
        self.default_sink.flush().await?;
        Ok(())
    }

    async fn close(&mut self) -> Result<()> {
        for rule in &mut self.rules {
            rule.sink.close().await?;
        }
        self.default_sink.close().await?;
        Ok(())
    }
}

/// Aggregates events before routing
pub struct AggregatingRouter {
    buffer: Vec<OutputEvent>,
    buffer_size: usize,
    flush_interval: Duration,
    sink: Box<dyn OutputSink>,
    stats: RouterStats,
    last_flush: std::time::Instant,
}

impl AggregatingRouter {
    /// Create a new aggregating router
    pub fn new(buffer_size: usize, flush_interval: Duration, sink: Box<dyn OutputSink>) -> Self {
        Self {
            buffer: Vec::with_capacity(buffer_size),
            buffer_size,
            flush_interval,
            sink,
            stats: RouterStats::default(),
            last_flush: std::time::Instant::now(),
        }
    }

    /// Flush the buffer to the sink
    async fn flush_buffer(&mut self) -> Result<()> {
        for event in self.buffer.drain(..) {
            self.sink.write(event).await?;
        }
        self.last_flush = std::time::Instant::now();
        Ok(())
    }

    /// Check if we should flush based on time
    fn should_flush_by_time(&self) -> bool {
        self.last_flush.elapsed() >= self.flush_interval
    }
}

#[async_trait]
impl OutputRouter for AggregatingRouter {
    async fn route(&mut self, event: OutputEvent) -> Result<()> {
        let start = std::time::Instant::now();

        self.buffer.push(event);

        // Flush if buffer is full or timeout reached
        if self.buffer.len() >= self.buffer_size || self.should_flush_by_time() {
            self.flush_buffer().await?;
        }

        self.stats.events_routed += 1;
        self.stats.total_latency_ms += start.elapsed().as_millis() as u64;

        Ok(())
    }

    fn stats(&self) -> RouterStats {
        self.stats.clone()
    }

    async fn flush(&mut self) -> Result<()> {
        self.flush_buffer().await?;
        self.sink.flush().await
    }

    async fn close(&mut self) -> Result<()> {
        self.flush_buffer().await?;
        self.sink.close().await
    }
}

/// A router that can be shared across threads
pub struct SharedRouter {
    inner: Arc<Mutex<Box<dyn OutputRouter>>>,
}

impl SharedRouter {
    /// Create a new shared router
    pub fn new(router: Box<dyn OutputRouter>) -> Self {
        Self {
            inner: Arc::new(Mutex::new(router)),
        }
    }

    /// Route an event
    pub async fn route(&self, event: OutputEvent) -> Result<()> {
        let mut router = self.inner.lock().await;
        router.route(event).await
    }

    /// Get statistics
    pub async fn stats(&self) -> RouterStats {
        let router = self.inner.lock().await;
        router.stats()
    }

    /// Flush the router
    pub async fn flush(&self) -> Result<()> {
        let mut router = self.inner.lock().await;
        router.flush().await
    }

    /// Close the router
    pub async fn close(&self) -> Result<()> {
        let mut router = self.inner.lock().await;
        router.close().await
    }
}

impl Clone for SharedRouter {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::filter::ComponentFilter;
    use crate::sink::BufferSink;
    use crate::{OutputSource, OutputStream};

    #[tokio::test]
    async fn test_broadcast_router() {
        let sink1 = Box::new(BufferSink::new(100));
        let sink2 = Box::new(BufferSink::new(100));

        let mut router = BroadcastRouter::new(vec![sink1, sink2]);

        let source = OutputSource::new("test", "container");
        let event = OutputEvent::runtime(source, OutputStream::stdout(b"test data".to_vec()), None);

        let result = router.route(event).await;
        assert!(result.is_ok());

        let stats = router.stats();
        assert_eq!(stats.events_routed, 1);
        assert_eq!(stats.routing_errors, 0);
    }

    #[tokio::test]
    async fn test_rule_based_router() {
        let backend_sink = Box::new(BufferSink::new(100));
        let default_sink = Box::new(BufferSink::new(100));

        let mut router = RuleBasedRouter::new(default_sink);
        router = router.add_rule(RoutingRule {
            filter: Box::new(ComponentFilter::allowlist(vec!["backend".to_string()])),
            sink: backend_sink,
            stop_on_match: true,
        });

        let source = OutputSource::new("backend", "container");
        let event =
            OutputEvent::runtime(source, OutputStream::stdout(b"backend data".to_vec()), None);

        let result = router.route(event).await;
        assert!(result.is_ok());

        let stats = router.stats();
        assert_eq!(stats.events_routed, 1);
    }

    #[tokio::test]
    async fn test_aggregating_router() {
        let sink = Box::new(BufferSink::new(100));
        let mut router = AggregatingRouter::new(5, Duration::from_secs(1), sink);

        // Add events below buffer size
        for i in 0..3 {
            let source = OutputSource::new("test", "container");
            let event = OutputEvent::runtime(
                source,
                OutputStream::stdout(format!("data {i}").into_bytes()),
                None,
            );
            router.route(event).await.unwrap();
        }

        // Buffer should not be flushed yet
        assert_eq!(router.buffer.len(), 3);

        // Flush manually
        router.flush().await.unwrap();
        assert_eq!(router.buffer.len(), 0);
    }
}
