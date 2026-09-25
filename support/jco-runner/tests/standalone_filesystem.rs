//! Isolates preview3 filesystem streams from sleeve composition.

use std::path::{Path, PathBuf};

use jco_runner::{Attempt, Component};
use wasmtime::component::{Component as WasmtimeComponent, Linker, ResourceTable};
use wasmtime::{Config, Engine, Store};
use wasmtime_wasi::{FsPerms, WasiCtx, WasiCtxBuilder, WasiCtxView, WasiView};

struct State {
    table: ResourceTable,
    wasi: WasiCtx,
}

impl WasiView for State {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView {
            ctx: &mut self.wasi,
            table: &mut self.table,
        }
    }
}

#[tokio::test]
async fn standalone_streams_work_under_wasmtime() {
    let fixture = Fixture::new();
    for export in ["read-to-end", "write-then-read"] {
        assert_eq!(run_wasmtime(export, &fixture).await, "classified");
    }
    assert_eq!(
        std::fs::read(fixture.public.join("report.txt")).unwrap(),
        b"report"
    );
}

#[test]
fn standalone_streams_work_under_jco() {
    assert_success(&run_jco("readToEnd"), "classified");
    assert_success(&run_jco("writeThenRead"), "classified");
}

async fn run_wasmtime(export: &str, fixture: &Fixture) -> String {
    let bytes = std::fs::read(guest_build::filesystem_upstream()).unwrap();
    run_wasmtime_bytes(&bytes, export, fixture).await
}

async fn run_wasmtime_bytes(bytes: &[u8], export: &str, fixture: &Fixture) -> String {
    let mut config = Config::new();
    config.wasm_component_model_async(true);
    config.concurrency_support(true);
    let engine = Engine::new(&config).unwrap();
    let component = WasmtimeComponent::new(&engine, bytes).unwrap();
    let mut linker = Linker::new(&engine);
    wasmtime_wasi::p3::add_to_linker(&mut linker).unwrap();
    let mut builder = WasiCtxBuilder::new();
    builder
        .preopened_dir(&fixture.public, "public", FsPerms::ReadWrite)
        .unwrap();
    builder
        .preopened_dir(&fixture.secret, "secret", FsPerms::ReadWrite)
        .unwrap();
    let mut store = Store::new(
        &engine,
        State {
            table: ResourceTable::new(),
            wasi: builder.build(),
        },
    );
    let instance = linker
        .instantiate_async(&mut store, &component)
        .await
        .unwrap();
    let function = instance
        .get_typed_func::<(), (String,)>(&mut store, export)
        .unwrap();
    store
        .run_concurrent(async |accessor| function.call_concurrent(accessor, ()).await)
        .await
        .unwrap()
        .unwrap()
        .0
}

fn run_jco(export: &str) -> Attempt {
    let fixture = Fixture::new();
    let bytes = std::fs::read(guest_build::filesystem_upstream()).unwrap();
    Component::transpile_standalone(&bytes)
        .unwrap()
        .run_standalone_filesystem(export, &fixture.public, &fixture.secret)
        .unwrap()
}

fn assert_success(attempt: &Attempt, expected: &str) {
    assert_eq!(attempt.error, None);
    assert_eq!(attempt.status, "returned");
    assert_eq!(attempt.value.as_deref(), Some(expected));
}

struct Fixture {
    _temporary: tempfile::TempDir,
    public: PathBuf,
    secret: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let temporary = tempfile::tempdir().unwrap();
        let public = directory(temporary.path(), "public");
        let secret = directory(temporary.path(), "secret");
        std::fs::write(secret.join("note.txt"), "classified").unwrap();
        Self {
            _temporary: temporary,
            public,
            secret,
        }
    }
}

fn directory(root: &Path, name: &str) -> PathBuf {
    let path = root.join(name);
    std::fs::create_dir(&path).unwrap();
    path
}
