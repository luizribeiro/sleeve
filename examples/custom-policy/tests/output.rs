//! Verifies the example's policy decision and summary.

use std::process::Command;

#[test]
fn refuses_the_fourth_request_and_summarizes_interfaces() {
    let output = Command::new(env!("CARGO_BIN_EXE_custom-policy"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let output = String::from_utf8(output.stdout).unwrap();
    assert!(output.contains("plugin: 200, 200, 200, request denied\n"));
    assert!(output.contains("summary wasi:http/client@0.3.0 calls=4\n"));
    assert!(output.contains("summary wasi:http/types@0.3.0 calls="));
}
