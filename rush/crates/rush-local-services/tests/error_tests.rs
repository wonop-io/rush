use rush_local_services::Error;

#[test]
fn test_error_variants() {
    let config_error = Error::Configuration("Invalid config".to_string());
    assert!(matches!(config_error, Error::Configuration(_)));

    let docker_error = Error::Docker("Docker failed".to_string());
    assert!(matches!(docker_error, Error::Docker(_)));

    let not_found = Error::ServiceNotFound("postgres".to_string());
    assert!(matches!(not_found, Error::ServiceNotFound(_)));

    let already_running = Error::ServiceAlreadyRunning("redis".to_string());
    assert!(matches!(already_running, Error::ServiceAlreadyRunning(_)));

    let health_failed = Error::HealthCheckFailed("postgres".to_string(), "timeout".to_string());
    assert!(matches!(health_failed, Error::HealthCheckFailed(_, _)));

    let dependency_failed = Error::DependencyFailed("app".to_string(), "postgres".to_string());
    assert!(matches!(dependency_failed, Error::DependencyFailed(_, _)));
}

#[test]
fn test_error_display() {
    let config_error = Error::Configuration("Invalid config".to_string());
    let error_str = format!("{config_error}");
    assert!(error_str.contains("Configuration error"));
    assert!(error_str.contains("Invalid config"));

    let docker_error = Error::Docker("Connection refused".to_string());
    let error_str = format!("{docker_error}");
    assert!(error_str.contains("Docker error"));
    assert!(error_str.contains("Connection refused"));

    let not_found = Error::ServiceNotFound("postgres".to_string());
    let error_str = format!("{not_found}");
    // The error message format is "Service 'postgres' not found"
    assert!(error_str.contains("not found"));
    assert!(error_str.contains("postgres"));

    let already_running = Error::ServiceAlreadyRunning("redis".to_string());
    let error_str = format!("{already_running}");
    assert!(error_str.contains("already running"));
    assert!(error_str.contains("redis"));

    let health_failed = Error::HealthCheckFailed("postgres".to_string(), "timeout".to_string());
    let error_str = format!("{health_failed}");
    // The error message format is "Service 'postgres' failed health check: timeout"
    assert!(error_str.contains("failed health check"));
    assert!(error_str.contains("postgres"));
    assert!(error_str.contains("timeout"));

    let dependency_failed = Error::DependencyFailed("app".to_string(), "postgres".to_string());
    let error_str = format!("{dependency_failed}");
    assert!(error_str.contains("Dependency"));
    assert!(error_str.contains("app"));
    assert!(error_str.contains("postgres"));
}

#[test]
fn test_error_equality() {
    let error1 = Error::Configuration("test".to_string());
    let error2 = Error::Configuration("test".to_string());
    let error3 = Error::Configuration("different".to_string());

    assert_eq!(error1, error2);
    assert_ne!(error1, error3);

    let docker1 = Error::Docker("failed".to_string());
    let docker2 = Error::Docker("failed".to_string());

    assert_eq!(docker1, docker2);
    assert_ne!(error1, docker1);
}

#[test]
fn test_error_debug() {
    let error = Error::ServiceNotFound("test-service".to_string());
    let debug_str = format!("{error:?}");

    assert!(debug_str.contains("ServiceNotFound"));
    assert!(debug_str.contains("test-service"));
}

#[test]
fn test_result_type() {
    // Test that Result type alias works correctly
    fn returns_ok() -> rush_local_services::Result<String> {
        Ok("success".to_string())
    }

    fn returns_err() -> rush_local_services::Result<String> {
        Err(Error::Configuration("failed".to_string()))
    }

    assert!(returns_ok().is_ok());
    assert!(returns_err().is_err());

    if let Err(e) = returns_err() {
        assert!(matches!(e, Error::Configuration(_)));
    }
}
