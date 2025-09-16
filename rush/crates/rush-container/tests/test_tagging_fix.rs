use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use tempfile::TempDir;
use rush_container::tagging::ImageTagGenerator;
use rush_build::{ComponentBuildSpec, BuildType};
use rush_toolchain::ToolchainContext;

#[test]
fn test_component_files_always_included_with_watch_patterns() {
    // Create a temporary directory structure
    let temp_dir = TempDir::new().unwrap();
    let base_path = temp_dir.path();

    // Create component directory and files
    let component_dir = base_path.join("app").join("frontend");
    fs::create_dir_all(&component_dir).unwrap();

    // Create component files that DON'T match watch patterns
    fs::write(component_dir.join("main.rs"), "fn main() {}").unwrap();
    fs::write(component_dir.join("lib.rs"), "pub fn lib() {}").unwrap();
    fs::write(component_dir.join("utils.rs"), "pub fn util() {}").unwrap();

    // Create files that DO match watch patterns
    fs::write(component_dir.join("test_api.rs"), "// api file").unwrap();
    fs::write(component_dir.join("test_screens.rs"), "// screens file").unwrap();

    // Create tag generator
    let toolchain = Arc::new(ToolchainContext::default());
    let tag_generator = ImageTagGenerator::new(toolchain, base_path.to_path_buf());

    // Create spec with restrictive watch patterns
    let spec = ComponentBuildSpec {
        component_name: "frontend".to_string(),
        build_type: BuildType::TrunkWasm {
            location: "app/frontend".to_string(),
            release: false,
            dockerfile: None,
            target_dir: None,
        },
        watch: Some(vec![
            "**/*_api".to_string(),
            "**/*_screens".to_string(),
        ]),
        environment: Default::default(),
        depends_on: vec![],
        dockerfile: None,
        config: Default::default(),
    };

    // Compute tag
    let tag1 = tag_generator.compute_tag(&spec).unwrap();

    // The tag should NOT be the empty hash
    assert_ne!(&tag1[..8.min(tag1.len())], "e3b0c442",
               "Tag should not be empty hash when component has files");

    // Modify a component file that doesn't match watch patterns
    fs::write(component_dir.join("main.rs"), "fn main() { println!(\"changed\"); }").unwrap();

    // Compute tag again
    let tag2 = tag_generator.compute_tag(&spec).unwrap();

    // Tags should be different because component file changed
    assert_ne!(tag1, tag2,
               "Tag should change when component files are modified, even if they don't match watch patterns");

    println!("Test passed! Tags: {} -> {}", tag1, tag2);
}

#[test]
fn test_empty_watch_patterns_dont_produce_empty_hash() {
    let temp_dir = TempDir::new().unwrap();
    let base_path = temp_dir.path();

    // Create component with files
    let component_dir = base_path.join("app").join("backend");
    fs::create_dir_all(&component_dir).unwrap();
    fs::write(component_dir.join("main.rs"), "fn main() {}").unwrap();

    let toolchain = Arc::new(ToolchainContext::default());
    let tag_generator = ImageTagGenerator::new(toolchain, base_path.to_path_buf());

    // Spec with watch patterns that match nothing
    let spec = ComponentBuildSpec {
        component_name: "backend".to_string(),
        build_type: BuildType::RustBinary {
            location: "app/backend".to_string(),
            release: false,
            dockerfile: None,
            target_dir: None,
        },
        watch: Some(vec!["nonexistent/*".to_string()]),
        environment: Default::default(),
        depends_on: vec![],
        dockerfile: None,
        config: Default::default(),
    };

    let tag = tag_generator.compute_tag(&spec).unwrap();

    // Should not be empty hash even with non-matching watch patterns
    assert_ne!(&tag[..8.min(tag.len())], "e3b0c442",
               "Should never produce empty string hash when component has files");

    println!("Test passed! Tag with non-matching patterns: {}", tag);
}

#[test]
fn test_watch_patterns_add_extra_files() {
    let temp_dir = TempDir::new().unwrap();
    let base_path = temp_dir.path();

    // Create component directory
    let component_dir = base_path.join("services").join("api");
    fs::create_dir_all(&component_dir).unwrap();
    fs::write(component_dir.join("main.rs"), "fn main() {}").unwrap();

    // Create external file that matches watch pattern
    let config_dir = base_path.join("config");
    fs::create_dir_all(&config_dir).unwrap();
    let config_file = config_dir.join("api_config.toml");
    fs::write(&config_file, "port = 8080").unwrap();

    let toolchain = Arc::new(ToolchainContext::default());
    let tag_generator = ImageTagGenerator::new(toolchain, base_path.to_path_buf());

    // Spec watching external config
    let spec = ComponentBuildSpec {
        component_name: "api".to_string(),
        build_type: BuildType::RustBinary {
            location: "services/api".to_string(),
            release: false,
            dockerfile: None,
            target_dir: None,
        },
        watch: Some(vec!["config/*_config.toml".to_string()]),
        environment: Default::default(),
        depends_on: vec![],
        dockerfile: None,
        config: Default::default(),
    };

    let tag1 = tag_generator.compute_tag(&spec).unwrap();

    // Modify the external config file
    fs::write(&config_file, "port = 9000").unwrap();

    let tag2 = tag_generator.compute_tag(&spec).unwrap();

    // Tag should change when watched external file changes
    assert_ne!(tag1, tag2,
               "Tag should change when watched external files are modified");

    println!("Test passed! External file change detected: {} -> {}", tag1, tag2);
}