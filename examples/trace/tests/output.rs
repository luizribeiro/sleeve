//! Verifies the human-readable example output.

use std::process::Command;

#[test]
fn prints_the_trace_before_the_summary() {
    let output = Command::new(env!("CARGO_BIN_EXE_trace")).output().unwrap();
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        concat!(
            "invocation start note-summary\n",
            "call 1 example:notes/notes@0.1.0.read name=today\n",
            "return 1 ok\n",
            "call 2 example:notes/notes@0.1.0.read name=project\n",
            "return 2 ok\n",
            "invocation end note-summary returned\n",
            "result: Call Ada at 10; The launch is Friday\n",
        )
    );
}
