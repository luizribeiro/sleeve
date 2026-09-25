//! Verifies the human-readable filesystem policy trace.

use std::process::Command;

#[test]
fn prints_open_and_closed_writer_decisions() {
    let output = Command::new(env!("CARGO_BIN_EXE_file-ifc"))
        .output()
        .unwrap();
    assert!(output.status.success());
    let output = String::from_utf8(output.stdout).unwrap();
    assert!(output.contains("scenario: read secret while a public writer is open\n"));
    assert!(output.contains("result: read refused\n"));
    assert!(output.contains("scenario: close the public writer before reading secret\n"));
    assert!(output.contains("result: classified\n"));
    assert!(output.contains("channel opened"));
    assert!(output.contains("channel closed"));
}
