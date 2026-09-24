//! Measures per-call overhead for direct, composed, and host-layer dispatch.

use std::sync::Arc;
use std::time::{Duration, Instant};

use wasm_component_middleware::{
    Call, Chain, Completion, Denied, Direction, InvocationContext, Layer, MiddlewareCtx,
    MiddlewareView, Outcome,
};
use wasmtime::component::{Accessor, Component, HasData, Linker};
use wasmtime::{Config, Engine, Store};

const CALLS: u32 = 100_000;
const RUNS: usize = 5;

macro_rules! timed_run {
    ($store:expr, |$accessor:ident| $call:expr) => {{
        let start = Instant::now();
        let total = $store
            .run_concurrent(async |$accessor| $call.await)
            .await
            .map_err(message)?
            .map_err(message)?;
        let elapsed = start.elapsed();
        assert_eq!(total, u64::from(CALLS));
        Ok::<Duration, anyhow::Error>(elapsed)
    }};
}

mod direct {
    wasmtime::component::bindgen!({
        path: ["../../wit/notes", "../../wit/platform", "wit"],
        world: "sleeve:benchmark/direct@0.1.0",
        imports: { "example:notes/notes@0.1.0": async | store },
        exports: { default: async | store },
        require_store_data_send: true,
    });
}

mod composed {
    wasmtime::component::bindgen!({
        path: ["../../wit/notes", "../../wit/platform", "wit"],
        world: "sleeve:benchmark/composed@0.1.0",
        imports: {
            "example:notes/notes@0.1.0": async | store,
            default: trappable,
        },
        exports: { default: async | store },
        require_store_data_send: true,
    });
}

struct DirectState;
struct DirectView;
struct DirectData;

impl HasData for DirectData {
    type Data<'a> = DirectView;
}

impl direct::example::notes::notes::Host for DirectView {}

impl direct::example::notes::notes::HostWithStore<DirectState> for DirectData {
    #[allow(clippy::unused_async_trait_impl)]
    async fn read(_: &Accessor<DirectState, Self>, _: String) -> String {
        "x".into()
    }
}

struct Pass;

impl Layer<LayeredState> for Pass {
    type Frame = ();

    fn before(&self, _: &mut LayeredState, _: &Call<'_>) -> Result<Self::Frame, Denied> {
        Ok(())
    }

    fn after(&self, _: &mut LayeredState, _: &Call<'_>, (): Self::Frame, _: Outcome<'_>) {}
}

struct LayeredState {
    middleware: MiddlewareCtx<Self>,
}

impl MiddlewareView for LayeredState {
    fn middleware(&mut self) -> &mut MiddlewareCtx<Self> {
        &mut self.middleware
    }
}

struct LayeredView;
struct LayeredData;

impl HasData for LayeredData {
    type Data<'a> = LayeredView;
}

impl direct::example::notes::notes::Host for LayeredView {}

impl direct::example::notes::notes::HostWithStore<LayeredState> for LayeredData {
    async fn read(store: &Accessor<LayeredState, Self>, _: String) -> String {
        let chain = store.with(|mut access| Arc::clone(access.data_mut().middleware().chain()));
        let call = Call::new(chain.next_id(), Direction::Import, "read")
            .in_interface("example:notes/notes", Some("0.1.0"));
        match chain
            .dispatch_async(store, &call, || async {
                Ok((String::from("x"), Completion::default()))
            })
            .await
        {
            Ok(value) => value,
            Err(_) => std::process::abort(),
        }
    }
}

struct ComposedState {
    audit_records: usize,
}

struct ComposedView<'a>(&'a mut ComposedState);
struct ComposedData;

impl HasData for ComposedData {
    type Data<'a> = ComposedView<'a>;
}

impl composed::example::notes::notes::Host for ComposedView<'_> {}

impl composed::example::notes::notes::HostWithStore<ComposedState> for ComposedData {
    #[allow(clippy::unused_async_trait_impl)]
    async fn read(_: &Accessor<ComposedState, Self>, _: String) -> String {
        "x".into()
    }
}

impl composed::sleeve::platform::audit::Host for ComposedView<'_> {
    fn log(&mut self, _: String) -> wasmtime::Result<()> {
        self.0.audit_records += 1;
        Ok(())
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    let engine = engine()?;
    let plugin = std::fs::read(guest_build::read_many())?;
    let direct_component = Component::from_binary(&engine, &plugin).map_err(message)?;
    let passthrough = std::fs::read(guest_build::passthrough_sleeve())?;
    let trace = std::fs::read(guest_build::trace_sleeve())?;
    let passthrough = composed_component(&engine, &plugin, &passthrough)?;
    let trace = composed_component(&engine, &plugin, &trace)?;

    let direct = direct_runs(&engine, &direct_component).await?;
    let sleeve = composed_runs(&engine, &passthrough).await?;
    let host_layer = layered_runs(&engine, &direct_component).await?;
    let traced = composed_runs(&engine, &trace).await?;

    println!("setup,median_ns_per_call");
    println!("direct,{:.2}", median(&direct));
    println!("passthrough-sleeve,{:.2}", median(&sleeve));
    println!("host-layer,{:.2}", median(&host_layer));
    println!("trace-sleeve,{:.2}", median(&traced));
    Ok(())
}

fn engine() -> anyhow::Result<Engine> {
    let mut config = Config::new();
    config.wasm_component_model_async(true);
    config.concurrency_support(true);
    Engine::new(&config).map_err(message)
}

fn composed_component(engine: &Engine, plugin: &[u8], sleeve: &[u8]) -> anyhow::Result<Component> {
    let bytes = sleeve_host::compose(plugin, sleeve, sleeve_host::sleeve_sha256(sleeve))?;
    Component::from_binary(engine, &bytes).map_err(message)
}

async fn direct_runs(engine: &Engine, component: &Component) -> anyhow::Result<Vec<Duration>> {
    let mut durations = Vec::with_capacity(RUNS);
    for _ in 0..RUNS {
        let mut linker = Linker::new(engine);
        direct::example::notes::notes::add_to_linker::<_, DirectData>(&mut linker, |_| DirectView)
            .map_err(message)?;
        let mut store = Store::new(engine, DirectState);
        let guest = direct::Direct::instantiate_async(&mut store, component, &linker)
            .await
            .map_err(message)?;
        durations.push(timed_run!(&mut store, |accessor| guest.call_run(accessor, CALLS))?);
    }
    Ok(durations)
}

async fn layered_runs(engine: &Engine, component: &Component) -> anyhow::Result<Vec<Duration>> {
    let mut durations = Vec::with_capacity(RUNS);
    for _ in 0..RUNS {
        let mut linker = Linker::new(engine);
        direct::example::notes::notes::add_to_linker::<_, LayeredData>(&mut linker, |_| {
            LayeredView
        })
        .map_err(message)?;
        let chain = Chain::builder().layer(Pass).build();
        let mut store = Store::new(
            engine,
            LayeredState {
                middleware: MiddlewareCtx::new(chain, InvocationContext::new("benchmark")),
            },
        );
        let guest = direct::Direct::instantiate_async(&mut store, component, &linker)
            .await
            .map_err(message)?;
        durations.push(timed_run!(&mut store, |accessor| guest.call_run(accessor, CALLS))?);
    }
    Ok(durations)
}

async fn composed_runs(engine: &Engine, component: &Component) -> anyhow::Result<Vec<Duration>> {
    let mut durations = Vec::with_capacity(RUNS);
    for _ in 0..RUNS {
        let mut linker = Linker::new(engine);
        composed::example::notes::notes::add_to_linker::<_, ComposedData>(&mut linker, |state| {
            ComposedView(state)
        })
        .map_err(message)?;
        composed::sleeve::platform::audit::add_to_linker::<_, ComposedData>(&mut linker, |state| {
            ComposedView(state)
        })
        .map_err(message)?;
        let mut store = Store::new(engine, ComposedState { audit_records: 0 });
        let guest = composed::Composed::instantiate_async(&mut store, component, &linker)
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
        durations.push(timed_run!(&mut store, |accessor| guest.call_run(accessor, CALLS))?);
    }
    Ok(durations)
}

fn median(durations: &[Duration]) -> f64 {
    let mut values = durations
        .iter()
        .map(|duration| duration.as_secs_f64() * 1_000_000_000.0 / f64::from(CALLS))
        .collect::<Vec<_>>();
    values.sort_by(f64::total_cmp);
    values[values.len() / 2]
}

fn message(error: impl std::fmt::Display) -> anyhow::Error {
    anyhow::anyhow!(error.to_string())
}
