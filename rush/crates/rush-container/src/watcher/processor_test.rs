//! Unit tests for the change processor

#[cfg(test)]
mod tests {
    use super::super::*;
    use std::path::PathBuf;
    use std::time::Duration;
    use tempfile::TempDir;

    #[test]
    fn test_change_processor_creation() {
        let temp_dir = TempDir::new().unwrap();
        let processor = ChangeProcessor::new(temp_dir.path(), 100);

        // Should start with no changes
        let files = processor.changed_files();
        assert!(files.lock().unwrap().is_empty());
    }

    #[test]
    fn test_add_change() {
        let temp_dir = TempDir::new().unwrap();
        let processor = ChangeProcessor::new(temp_dir.path(), 100);

        // Add a change
        let test_file = temp_dir.path().join("test.rs");
        processor.add_change(test_file.clone());

        // Should have one change
        let files = processor.changed_files();
        let files = files.lock().unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0], test_file);
    }

    #[test]
    fn test_ignore_hidden_files() {
        let temp_dir = TempDir::new().unwrap();
        let processor = ChangeProcessor::new(temp_dir.path(), 100);

        // Hidden files should be ignored
        let hidden_file = temp_dir.path().join(".hidden");
        processor.add_change(hidden_file);

        // Should have no changes
        let files = processor.changed_files();
        assert!(files.lock().unwrap().is_empty());
    }

    #[test]
    fn test_ignore_temp_files() {
        let temp_dir = TempDir::new().unwrap();
        let processor = ChangeProcessor::new(temp_dir.path(), 100);

        // Temp files should be ignored
        let temp_file = temp_dir.path().join("file.tmp");
        processor.add_change(temp_file);

        let backup_file = temp_dir.path().join("file~");
        processor.add_change(backup_file);

        // Should have no changes
        let files = processor.changed_files();
        assert!(files.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_process_pending_changes_empty() {
        let temp_dir = TempDir::new().unwrap();
        let processor = ChangeProcessor::new(temp_dir.path(), 100);

        // Process with no changes
        let result = processor.process_pending_changes().await.unwrap();
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn test_process_pending_changes_with_files() {
        let temp_dir = TempDir::new().unwrap();
        let processor = ChangeProcessor::new(temp_dir.path(), 100);

        // Add some changes
        let file1 = temp_dir.path().join("file1.rs");
        let file2 = temp_dir.path().join("file2.rs");
        processor.add_change(file1.clone());
        processor.add_change(file2.clone());

        // Process changes
        let result = processor.process_pending_changes().await.unwrap();
        assert_eq!(result.len(), 2);
        assert!(result.contains(&file1));
        assert!(result.contains(&file2));

        // Changes should be cleared after processing
        let files = processor.changed_files();
        assert!(files.lock().unwrap().is_empty());
    }
}