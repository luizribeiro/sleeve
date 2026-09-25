//! Isolates concurrent asynchronous exports under jco.

use jco_runner::Component;

#[test]
#[ignore = "jco 1.35.0: RuntimeError: wasm trap: deadlock detected: event loop cannot make further progress"]
fn concurrent_exports_advance_one_instance_under_jco() {
    let bytes = std::fs::read(guest_build::concurrent_exports()).unwrap();
    let attempt = Component::transpile_standalone(&bytes)
        .unwrap()
        .run_concurrent_exports()
        .unwrap();
    assert_eq!(attempt.error, None);
    assert_eq!(attempt.status, "returned");
    assert_eq!(attempt.value.as_deref(), Some("7"));
}
