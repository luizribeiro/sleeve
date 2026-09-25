//! Verifies the human-readable jco policy trace.

use std::process::Command;

#[test]
fn prints_both_ordering_decisions() {
    let output = Command::new(env!("CARGO_BIN_EXE_jco")).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let output = String::from_utf8(output.stdout).unwrap();
    assert!(output.contains("scenario: read note, then fetch\n"));
    assert!(output.contains("return 3 denied\n  decision: denied\n"));
    assert!(output.contains("scenario: fetch, then read note\n"));
    assert!(output.contains("decision: allowed (200 classified)\n"));
}
