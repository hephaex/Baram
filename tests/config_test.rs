//! Tests for config module

use std::path::PathBuf;

// Note: These tests will be expanded when config loading is fully implemented
// For now, we test the basic structure compilation

#[test]
fn test_default_output_dir() {
    let expected = PathBuf::from("./output/raw");
    // This will be tested against actual config loading later
    assert!(expected.to_str().unwrap().contains("output"));
}

#[test]
fn test_config_file_exists() {
    let config_path = std::path::Path::new("config.toml");
    assert!(
        config_path.exists(),
        "config.toml should exist in project root"
    );
}

#[test]
fn test_config_toml_readable() {
    let content =
        std::fs::read_to_string("config.toml").expect("Should be able to read config.toml");

    // Basic validation - should have expected sections
    assert!(
        content.contains("[app]"),
        "config.toml should have [app] section"
    );
    assert!(
        content.contains("[crawler]"),
        "config.toml should have [crawler] section"
    );
    assert!(
        content.contains("[storage]"),
        "config.toml should have [storage] section"
    );
    assert!(
        content.contains("[logging]"),
        "config.toml should have [logging] section"
    );
}
