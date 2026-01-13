use assert_cmd::cargo::*;
use serde_json::Value;
use std::fs;

#[test]
fn parses_file_and_outputs_expected_json() -> Result<(), Box<dyn std::error::Error>> {
    let input_path = "./testdata/v5.1.1.ftrace";
    let expected_json = fs::read_to_string("testdata/bench_v5_1_1.json")?;

    let mut cmd = cargo_bin_cmd!("hitrace-bench");
    let assert = cmd
        .arg("--bencher")
        .arg("--trace-file")
        .arg(input_path)
        .assert();

    let output = assert.success().get_output().stdout.clone();
    let actual_json: Value = serde_json::from_slice(&output)?;
    let expected_json: Value = serde_json::from_str(&expected_json)?;

    assert_eq!(actual_json, expected_json);
    Ok(())
}
