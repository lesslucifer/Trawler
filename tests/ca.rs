use std::fs;

use tempfile::tempdir;
use trawler::ca::load_or_create;
use trawler::Config;

#[test]
#[should_panic(expected = "partial CA state")]
fn cert_without_key_refuses_to_regenerate() {
    let dir = tempdir().unwrap();
    let config = Config { ca_dir: dir.path().to_path_buf(), ..Default::default() };
    load_or_create(&config);
    fs::remove_file(config.ca_key_path()).unwrap();
    load_or_create(&config);
}

#[test]
#[should_panic(expected = "partial CA state")]
fn key_without_cert_refuses_to_regenerate() {
    let dir = tempdir().unwrap();
    let config = Config { ca_dir: dir.path().to_path_buf(), ..Default::default() };
    load_or_create(&config);
    fs::remove_file(config.ca_cert_path()).unwrap();
    load_or_create(&config);
}

#[test]
fn first_run_generates_then_second_run_loads_identical() {
    let dir = tempdir().unwrap();
    let config = Config { ca_dir: dir.path().to_path_buf(), ..Default::default() };

    let first = load_or_create(&config);
    assert!(config.ca_cert_path().exists(), "cert not persisted");
    assert!(config.ca_key_path().exists(), "key not persisted");
    assert!(first.pems.cert.contains("BEGIN CERTIFICATE"));

    let key = fs::read_to_string(config.ca_key_path()).unwrap();
    assert!(key.contains("PRIVATE KEY"));

    let second = load_or_create(&config);
    assert_eq!(first.pems.cert, second.pems.cert, "second run must load, not regenerate");
}
