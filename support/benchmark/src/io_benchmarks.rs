use std::sync::Arc;
use std::time::{Duration, Instant};

use http_scenarios::LocalServer;
use wasm_component_middleware::{
    Call, Chain, Denied, InvocationContext, Layer, MiddlewareCtx, MiddlewareView, Outcome,
};
use wasm_component_middleware_wasi_http::DefaultHooks;
use wasmtime::component::{Accessor, Component, HasData, Linker, ResourceTable};
use wasmtime::{Engine, Store};
use wasmtime_wasi::{FsPerms, WasiCtx, WasiCtxBuilder, WasiCtxView, WasiView};
use wasmtime_wasi_http::{WasiHttpCtx, WasiHttpCtxView, WasiHttpView};

const HTTP_CALLS: u32 = 500;
const RUNS: usize = 5;
const FILE_SIZE: usize = 1024 * 1024;

mod http_direct {
    wasmtime::component::bindgen!({
        path: ["../../wit/notes", "../../wit/http", "../../wit/clocks", "../../wit/filesystem", "../../wit/platform", "io-wit"],
        world: "sleeve:benchmark-io/http-direct@0.1.0",
        imports: { "example:notes/notes@0.1.0": async | store, default: trappable },
        exports: { default: async | store },
        require_store_data_send: true,
    });
}

mod http_composed {
    wasmtime::component::bindgen!({
        path: ["../../wit/notes", "../../wit/http", "../../wit/clocks", "../../wit/filesystem", "../../wit/platform", "io-wit"],
        world: "sleeve:benchmark-io/http-composed@0.1.0",
        imports: { "example:notes/notes@0.1.0": async | store, default: trappable },
        exports: { default: async | store },
        require_store_data_send: true,
    });
}

mod file_direct {
    wasmtime::component::bindgen!({
        path: ["../../wit/notes", "../../wit/http", "../../wit/clocks", "../../wit/filesystem", "../../wit/platform", "io-wit"],
        world: "sleeve:benchmark-io/file-direct@0.1.0",
        imports: { default: trappable },
        exports: { default: async | store },
        with: { "wasi": wasmtime_wasi::p3::bindings },
        require_store_data_send: true,
    });
}

mod file_composed {
    wasmtime::component::bindgen!({
        path: ["../../wit/notes", "../../wit/http", "../../wit/clocks", "../../wit/filesystem", "../../wit/platform", "io-wit"],
        world: "sleeve:benchmark-io/file-composed@0.1.0",
        imports: { default: trappable },
        exports: { default: async | store },
        with: { "wasi": wasmtime_wasi::p3::bindings },
        require_store_data_send: true,
    });
}

struct Pass;

struct State {
    middleware: MiddlewareCtx<Self>,
    table: ResourceTable,
    wasi: WasiCtx,
    http: WasiHttpCtx,
    hooks: DefaultHooks,
    audit: Vec<String>,
    allowed_origins: Vec<String>,
    preopen_labels: Vec<(String, String)>,
}

struct StateView<'a>(&'a mut State);
struct StateData;

impl Layer<State> for Pass {
    type Frame = ();

    fn before(&self, _: &mut State, _: &Call<'_>) -> Result<(), Denied> {
        Ok(())
    }

    fn after(&self, _: &mut State, _: &Call<'_>, (): (), _: Outcome<'_>) {}
}

impl MiddlewareView for State {
    fn middleware(&mut self) -> &mut MiddlewareCtx<Self> {
        &mut self.middleware
    }
}

impl WasiView for State {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView {
            ctx: &mut self.wasi,
            table: &mut self.table,
        }
    }
}

impl WasiHttpView for State {
    fn http(&mut self) -> WasiHttpCtxView<'_> {
        WasiHttpCtxView {
            ctx: &mut self.http,
            table: &mut self.table,
            hooks: &mut self.hooks,
        }
    }
}

impl HasData for StateData {
    type Data<'a> = StateView<'a>;
}

macro_rules! notes_host {
    ($bindings:ident) => {
        impl $bindings::example::notes::notes::Host for StateView<'_> {}

        impl $bindings::example::notes::notes::HostWithStore<State> for StateData {
            #[allow(clippy::unused_async_trait_impl)]
            async fn read(_: &Accessor<State, Self>, _: String) -> String {
                "benchmark note".into()
            }
        }
    };
}

notes_host!(http_direct);
notes_host!(http_composed);

impl http_composed::sleeve::platform::audit::Host for StateView<'_> {
    fn log(&mut self, record: String) -> wasmtime::Result<()> {
        self.0.audit.push(record);
        Ok(())
    }
}

macro_rules! settings_host {
    ($bindings:ident) => {
        impl $bindings::sleeve::platform::settings::Host for StateView<'_> {
            fn request_body_limit(&mut self) -> wasmtime::Result<u64> {
                Ok(u64::MAX)
            }

            fn allowed_origins(&mut self) -> wasmtime::Result<Vec<String>> {
                Ok(self.0.allowed_origins.clone())
            }

            fn preopen_label(&mut self, name: String) -> wasmtime::Result<String> {
                Ok(self
                    .0
                    .preopen_labels
                    .iter()
                    .find_map(|(candidate, label)| (candidate == &name).then(|| label.clone()))
                    .unwrap_or_default())
            }
        }
    };
}

settings_host!(http_composed);
settings_host!(file_composed);

#[derive(Clone, Copy)]
enum Mode {
    Direct,
    HostGate,
    Sleeve,
}

impl Mode {
    const ALL: [Self; 3] = [Self::Direct, Self::HostGate, Self::Sleeve];

    const fn name(self) -> &'static str {
        match self {
            Self::Direct => "direct",
            Self::HostGate => "host-gate",
            Self::Sleeve => "ifc-sleeve",
        }
    }
}

pub(super) async fn run(engine: &Engine) -> anyhow::Result<()> {
    let server = LocalServer::start()?;
    let http_plugin = std::fs::read(guest_build::http_scenarios())?;
    let ifc = std::fs::read(guest_build::ifc_sleeve())?;
    let mut http = Vec::new();
    for mode in Mode::ALL {
        let durations = http_runs(engine, mode, &http_plugin, &ifc, &server).await?;
        http.push((mode, median_per(&durations, HTTP_CALLS)));
    }
    let direct = http[0].1;
    println!("http_setup,median_us_per_send,delta_us_vs_direct");
    for (mode, duration) in http {
        println!("{},{duration:.2},{:.2}", mode.name(), duration - direct);
    }
    server.check()?;

    let file_plugin = std::fs::read(guest_build::file_scenarios())?;
    let file_ifc = std::fs::read(guest_build::file_ifc_sleeve())?;
    println!("file_operation,setup,median_us_per_mib");
    for scenario in [10_u8, 11] {
        let operation = if scenario == 10 { "write" } else { "read" };
        for mode in Mode::ALL {
            let durations = file_runs(engine, mode, &file_plugin, &file_ifc, scenario).await?;
            println!("{operation},{},{:.2}", mode.name(), median_us(&durations));
        }
    }
    Ok(())
}

async fn http_runs(
    engine: &Engine,
    mode: Mode,
    plugin: &[u8],
    sleeve: &[u8],
    server: &LocalServer,
) -> anyhow::Result<Vec<Duration>> {
    let component = match mode {
        Mode::Sleeve => {
            let bytes = sleeve_host::compose(plugin, sleeve, sleeve_host::sleeve_sha256(sleeve))?;
            Component::from_binary(engine, &bytes).map_err(message)?
        }
        Mode::Direct | Mode::HostGate => Component::from_binary(engine, plugin).map_err(message)?,
    };
    let mut durations = Vec::with_capacity(RUNS);
    for _ in 0..RUNS {
        let chain = chain(mode);
        let mut store = Store::new(engine, state(chain, [server.origin()], None)?);
        let elapsed = match mode {
            Mode::Direct | Mode::HostGate => {
                http_host_run(engine, &component, &mut store, mode, server).await?
            }
            Mode::Sleeve => http_sleeve_run(engine, &component, &mut store, server).await?,
        };
        durations.push(elapsed);
    }
    Ok(durations)
}

async fn http_host_run(
    engine: &Engine,
    component: &Component,
    store: &mut Store<State>,
    mode: Mode,
    server: &LocalServer,
) -> anyhow::Result<Duration> {
    let mut linker = Linker::new(engine);
    http_direct::example::notes::notes::add_to_linker::<_, StateData>(&mut linker, |state| {
        StateView(state)
    })
    .map_err(message)?;
    if matches!(mode, Mode::Direct) {
        wasmtime_wasi_http::p3::add_to_linker(&mut linker).map_err(message)?;
    } else {
        wasm_component_middleware_wasi_http::p3::add_to_linker(&mut linker).map_err(message)?;
    }
    let guest = http_direct::HttpDirect::instantiate_async(&mut *store, component, &linker)
        .await
        .map_err(message)?;
    let start = Instant::now();
    let value = store
        .run_concurrent(async |accessor| {
            guest
                .call_run(accessor, 8, server.authority().into(), HTTP_CALLS)
                .await
        })
        .await
        .map_err(message)?
        .map_err(message)?;
    validate_http(&value);
    Ok(start.elapsed())
}

async fn http_sleeve_run(
    engine: &Engine,
    component: &Component,
    store: &mut Store<State>,
    server: &LocalServer,
) -> anyhow::Result<Duration> {
    let mut linker = Linker::new(engine);
    http_composed::example::notes::notes::add_to_linker::<_, StateData>(&mut linker, |state| {
        StateView(state)
    })
    .map_err(message)?;
    http_composed::sleeve::platform::audit::add_to_linker::<_, StateData>(&mut linker, |state| {
        StateView(state)
    })
    .map_err(message)?;
    http_composed::sleeve::platform::settings::add_to_linker::<_, StateData>(
        &mut linker,
        |state| StateView(state),
    )
    .map_err(message)?;
    wasmtime_wasi_http::p3::add_to_linker(&mut linker).map_err(message)?;
    let guest = http_composed::HttpComposed::instantiate_async(&mut *store, component, &linker)
        .await
        .map_err(message)?;
    store
        .run_concurrent(async |accessor| {
            guest
                .sleeve_platform_lifecycle()
                .call_start(accessor, "benchmark".into())
                .await
        })
        .await
        .map_err(message)?
        .map_err(message)?;
    let start = Instant::now();
    let value = store
        .run_concurrent(async |accessor| {
            let anchor = guest.sleeve_platform_anchor().call_run(accessor);
            let invocation = async {
                let value = guest
                    .call_run(accessor, 8, server.authority().into(), HTTP_CALLS)
                    .await;
                let stopped = guest.sleeve_platform_anchor().call_stop(accessor).await;
                let value = value?;
                stopped?;
                Ok::<String, wasmtime::Error>(value)
            };
            let (anchor, value) = tokio::join!(anchor, invocation);
            anchor?;
            value
        })
        .await
        .map_err(message)?
        .map_err(message)?;
    validate_http(&value);
    Ok(start.elapsed())
}

fn validate_http(value: &str) {
    assert_eq!(value.split(", ").count(), HTTP_CALLS as usize);
    assert!(value.split(", ").all(|status| status == "200"));
}

async fn file_runs(
    engine: &Engine,
    mode: Mode,
    plugin: &[u8],
    sleeve: &[u8],
    scenario: u8,
) -> anyhow::Result<Vec<Duration>> {
    let component = match mode {
        Mode::Sleeve => {
            let bytes = sleeve_host::compose(plugin, sleeve, sleeve_host::sleeve_sha256(sleeve))?;
            Component::from_binary(engine, &bytes).map_err(message)?
        }
        Mode::Direct | Mode::HostGate => Component::from_binary(engine, plugin).map_err(message)?,
    };
    let mut durations = Vec::with_capacity(RUNS);
    for _ in 0..RUNS {
        let public = tempfile::tempdir()?;
        let secret = tempfile::tempdir()?;
        std::fs::write(public.path().join("benchmark.bin"), vec![b'x'; FILE_SIZE])?;
        let chain = chain(mode);
        let mut store = Store::new(engine, state(chain, [], Some((&public, &secret)))?);
        let elapsed = match mode {
            Mode::Direct | Mode::HostGate => {
                let mut linker = Linker::new(engine);
                if matches!(mode, Mode::Direct) {
                    wasmtime_wasi::p3::add_to_linker(&mut linker).map_err(message)?;
                } else {
                    wasm_component_middleware_wasi::p3::add_to_linker_with_stream_relay(
                        &mut linker,
                        wasm_component_middleware_wasi::p3::StreamRelay::default(),
                    )
                    .map_err(message)?;
                }
                let guest =
                    file_direct::FileDirect::instantiate_async(&mut store, &component, &linker)
                        .await
                        .map_err(message)?;
                let start = Instant::now();
                let value = store
                    .run_concurrent(async |accessor| guest.call_run(accessor, scenario).await)
                    .await
                    .map_err(message)?
                    .map_err(message)?;
                assert_eq!(value, FILE_SIZE.to_string());
                start.elapsed()
            }
            Mode::Sleeve => {
                let mut linker = Linker::new(engine);
                file_composed::sleeve::platform::settings::add_to_linker::<_, StateData>(
                    &mut linker,
                    |state| StateView(state),
                )
                .map_err(message)?;
                wasmtime_wasi::p3::add_to_linker(&mut linker).map_err(message)?;
                let guest =
                    file_composed::FileComposed::instantiate_async(&mut store, &component, &linker)
                        .await
                        .map_err(message)?;
                store
                    .run_concurrent(async |accessor| {
                        guest
                            .sleeve_platform_lifecycle()
                            .call_start(accessor, "benchmark".into())
                            .await
                    })
                    .await
                    .map_err(message)?
                    .map_err(message)?;
                let start = Instant::now();
                let value = store
                    .run_concurrent(async |accessor| {
                        let anchor = guest.sleeve_platform_anchor().call_run(accessor);
                        let invocation = async {
                            let value = guest.call_run(accessor, scenario).await;
                            let stopped = guest.sleeve_platform_anchor().call_stop(accessor).await;
                            let value = value?;
                            stopped?;
                            Ok::<String, wasmtime::Error>(value)
                        };
                        let (anchor, value) = tokio::join!(anchor, invocation);
                        anchor?;
                        value
                    })
                    .await
                    .map_err(message)?
                    .map_err(message)?;
                assert_eq!(value, FILE_SIZE.to_string());
                start.elapsed()
            }
        };
        durations.push(elapsed);
    }
    Ok(durations)
}

fn chain(mode: Mode) -> Arc<Chain<State>> {
    if matches!(mode, Mode::HostGate) {
        Chain::builder().layer(Pass).build()
    } else {
        Chain::builder().build()
    }
}

fn state(
    chain: Arc<Chain<State>>,
    origins: impl IntoIterator<Item = String>,
    directories: Option<(&tempfile::TempDir, &tempfile::TempDir)>,
) -> anyhow::Result<State> {
    let mut wasi = WasiCtxBuilder::new();
    let mut labels = Vec::new();
    if let Some((public, secret)) = directories {
        wasi.preopened_dir(public.path(), "public", FsPerms::ReadWrite)
            .map_err(message)?;
        wasi.preopened_dir(secret.path(), "secret", FsPerms::ReadWrite)
            .map_err(message)?;
        labels.push(("public".into(), "public".into()));
        labels.push(("secret".into(), "secret".into()));
    }
    Ok(State {
        middleware: MiddlewareCtx::new(chain, InvocationContext::new("benchmark")),
        table: ResourceTable::new(),
        wasi: wasi.build(),
        http: WasiHttpCtx::new(),
        hooks: DefaultHooks,
        audit: Vec::new(),
        allowed_origins: origins.into_iter().collect(),
        preopen_labels: labels,
    })
}

fn median_per(durations: &[Duration], calls: u32) -> f64 {
    let mut values = durations
        .iter()
        .map(|duration| duration.as_secs_f64() * 1_000_000.0 / f64::from(calls))
        .collect::<Vec<_>>();
    values.sort_by(f64::total_cmp);
    values[values.len() / 2]
}

fn median_us(durations: &[Duration]) -> f64 {
    let mut values = durations
        .iter()
        .map(|duration| duration.as_secs_f64() * 1_000_000.0)
        .collect::<Vec<_>>();
    values.sort_by(f64::total_cmp);
    values[values.len() / 2]
}

fn message(error: impl std::fmt::Display) -> anyhow::Error {
    anyhow::anyhow!(error.to_string())
}
