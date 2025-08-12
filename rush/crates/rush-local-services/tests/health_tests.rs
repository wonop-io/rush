use rush_local_services::{HealthCheck, HealthStatus};
use std::time::Duration;

#[test]
fn test_health_check_creation() {
    let check = HealthCheck::new("pg_isready -U postgres".to_string());
    
    assert_eq!(check.command, "pg_isready -U postgres");
    assert_eq!(check.interval, Duration::from_secs(5));
    assert_eq!(check.timeout, Duration::from_secs(3));
    assert_eq!(check.retries, 10);
    assert_eq!(check.start_period, Duration::from_secs(10));
}

#[test]
fn test_health_check_with_settings() {
    let mut check = HealthCheck::new("redis-cli ping".to_string());
    check.interval = Duration::from_secs(2);
    check.timeout = Duration::from_secs(1);
    check.retries = 5;
    check.start_period = Duration::from_secs(5);
    
    assert_eq!(check.command, "redis-cli ping");
    assert_eq!(check.interval, Duration::from_secs(2));
    assert_eq!(check.timeout, Duration::from_secs(1));
    assert_eq!(check.retries, 5);
    assert_eq!(check.start_period, Duration::from_secs(5));
}

#[test]
fn test_health_status_variants() {
    let healthy = HealthStatus::Healthy;
    assert!(matches!(healthy, HealthStatus::Healthy));
    
    let unhealthy = HealthStatus::Unhealthy("Connection refused".to_string());
    assert!(matches!(unhealthy, HealthStatus::Unhealthy(_)));
    
    let not_running = HealthStatus::NotRunning;
    assert!(matches!(not_running, HealthStatus::NotRunning));
    
    let unknown = HealthStatus::Unknown;
    assert!(matches!(unknown, HealthStatus::Unknown));
}

#[test]
fn test_health_status_equality() {
    assert_eq!(HealthStatus::Healthy, HealthStatus::Healthy);
    assert_eq!(HealthStatus::NotRunning, HealthStatus::NotRunning);
    assert_eq!(HealthStatus::Unknown, HealthStatus::Unknown);
    
    assert_eq!(
        HealthStatus::Unhealthy("error".to_string()),
        HealthStatus::Unhealthy("error".to_string())
    );
    
    assert_ne!(
        HealthStatus::Unhealthy("error1".to_string()),
        HealthStatus::Unhealthy("error2".to_string())
    );
    
    assert_ne!(HealthStatus::Healthy, HealthStatus::NotRunning);
}

#[test]
fn test_health_check_clone() {
    let original = HealthCheck::new("test command".to_string());
    let cloned = original.clone();
    
    assert_eq!(original.command, cloned.command);
    assert_eq!(original.interval, cloned.interval);
    assert_eq!(original.timeout, cloned.timeout);
    assert_eq!(original.retries, cloned.retries);
    assert_eq!(original.start_period, cloned.start_period);
}

#[test]
fn test_health_check_defaults() {
    let check = HealthCheck::new("test".to_string());
    
    // Verify default values match expected settings
    assert_eq!(check.interval.as_secs(), 5);
    assert_eq!(check.timeout.as_secs(), 3);
    assert_eq!(check.retries, 10);
    assert_eq!(check.start_period.as_secs(), 10);
}

#[test]
fn test_health_status_debug() {
    let healthy = HealthStatus::Healthy;
    let debug_str = format!("{:?}", healthy);
    assert!(debug_str.contains("Healthy"));
    
    let unhealthy = HealthStatus::Unhealthy("test error".to_string());
    let debug_str = format!("{:?}", unhealthy);
    assert!(debug_str.contains("Unhealthy"));
    assert!(debug_str.contains("test error"));
}