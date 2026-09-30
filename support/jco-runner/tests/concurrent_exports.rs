//! Checks that one instance advances two concurrent asynchronous exports on both hosts.

use std::time::Duration;

use jco_runner::Component;
use wasmtime::component::{Component as WasmtimeComponent, Linker};
use wasmtime::{Config, Engine, Store};

mod bindings {
    wasmtime::component::bindgen!({
        path: "../../guests/plugins/concurrent-exports/wit",
        world: "example:concurrent-exports/probe@0.1.0",
        exports: { default: async | store },
    });
}

#[test]
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

#[tokio::test]
async fn concurrent_exports_advance_one_instance_under_wasmtime() {
    let mut config = Config::new();
    config.wasm_component_model_async(true);
    config.concurrency_support(true);
    let engine = Engine::new(&config).unwrap();
    let component =
        WasmtimeComponent::from_file(&engine, guest_build::concurrent_exports()).unwrap();
    let mut store = Store::new(&engine, ());
    let probe = bindings::Probe::instantiate_async(&mut store, &component, &Linker::new(&engine))
        .await
        .unwrap();
    let run = store.run_concurrent(async |accessor| {
        let (value, helped) = tokio::join!(probe.call_run(accessor), probe.call_helper(accessor));
        let mut unused = helped.unwrap();
        accessor
            .with(|mut access| unused.close(&mut access))
            .unwrap();
        value.unwrap()
    });
    let value = tokio::time::timeout(Duration::from_secs(30), run)
        .await
        .expect("the exports did not both complete")
        .unwrap();
    assert_eq!(value, 7);
}
