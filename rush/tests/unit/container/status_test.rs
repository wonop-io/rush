use rush_cli::container::status::Status;

#[test]
fn test_status_equality() {
    assert_eq!(Status::Awaiting, Status::Awaiting);
    assert_eq!(Status::InProgress, Status::InProgress);
    assert_eq!(Status::StartupCompleted, Status::StartupCompleted);
    assert_eq!(Status::Reinitializing, Status::Reinitializing);
    assert_eq!(Status::Terminate, Status::Terminate);
    assert_eq!(Status::Finished(0), Status::Finished(0));
    assert_eq!(Status::Finished(1), Status::Finished(1));
    
    assert_ne!(Status::Awaiting, Status::InProgress);
    assert_ne!(Status::InProgress, Status::StartupCompleted);
    assert_ne!(Status::StartupCompleted, Status::Reinitializing);
    assert_ne!(Status::Reinitializing, Status::Terminate);
    assert_ne!(Status::Terminate, Status::Finished(0));
    assert_ne!(Status::Finished(0), Status::Finished(1));
}

#[test]
fn test_status_debug() {
    // Test that Debug is implemented
    let status = Status::Awaiting;
    let debug_str = format!("{:?}", status);
    assert_eq!(debug_str, "Awaiting");
    
    let status = Status::Finished(42);
    let debug_str = format!("{:?}", status);
    assert_eq!(debug_str, "Finished(42)");
}

#[test]
fn test_status_clone() {
    let status = Status::Awaiting;
    let cloned = status.clone();
    assert_eq!(status, cloned);
    
    let status = Status::Finished(123);
    let cloned = status.clone();
    assert_eq!(status, cloned);
}

#[test]
fn test_status_partial_eq() {
    let status1 = Status::InProgress;
    let status2 = Status::InProgress;
    let status3 = Status::StartupCompleted;
    
    assert!(status1 == status2);
    assert!(!(status1 == status3));
    assert!(status1 != status3);
    assert!(!(status1 != status2));
}