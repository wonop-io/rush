#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    Awaiting,
    InProgress,
    StartupCompleted,
    Reinitializing,
    Finished(i32),
    Terminate,
}
