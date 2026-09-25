//! Measures per-call overhead for direct, composed, and host-layer dispatch.

use std::sync::Arc;
use std::time::{Duration, Instant};

use wac_graph::{CompositionGraph, EncodeOptions, types::Package};
use wasm_component_middleware::{
    Call, Chain, Completion, Denied, Direction, InvocationContext, Layer, MiddlewareCtx,
    MiddlewareView, Outcome,
};
use wasmtime::component::{Accessor, Component, HasData, Linker};
use wasmtime::{Config, Engine, Store};

mod io_benchmarks;

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
    let external_sleeve = std::fs::read(guest_build::external_policy_sleeve())?;
    let trace_policy = std::fs::read(guest_build::trace_policy())?;
    let external_trace = compose_policy(&external_sleeve, &trace_policy)?;
    let passthrough = composed_component(&engine, &plugin, &passthrough)?;
    let trace = composed_component(&engine, &plugin, &trace)?;
    let external_trace = composed_component(&engine, &plugin, &external_trace)?;

    let direct = direct_runs(&engine, &direct_component).await?;
    let sleeve = composed_runs(&engine, &passthrough).await?;
    let host_layer = layered_runs(&engine, &direct_component).await?;
    let traced = composed_runs(&engine, &trace).await?;
    let external_traced = composed_runs(&engine, &external_trace).await?;

    println!("setup,median_ns_per_call");
    println!("direct,{:.2}", median(&direct));
    println!("passthrough-sleeve,{:.2}", median(&sleeve));
    println!("host-layer,{:.2}", median(&host_layer));
    println!("trace-sleeve,{:.2}", median(&traced));
    println!("external-trace-policy,{:.2}", median(&external_traced));
    io_benchmarks::run(&engine).await?;
    Ok(())
}

fn compose_policy(sleeve: &[u8], policy: &[u8]) -> anyhow::Result<Vec<u8>> {
    let mut graph = CompositionGraph::new();
    let sleeve =
        Package::from_bytes("sleeve:external", None, sleeve, graph.types_mut()).map_err(message)?;
    let policy =
        Package::from_bytes("policy:trace", None, policy, graph.types_mut()).map_err(message)?;
    let exports = graph.types()[sleeve.ty()]
        .exports
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    let sleeve_id = graph.register_package(sleeve).map_err(message)?;
    let policy_id = graph.register_package(policy).map_err(message)?;
    let policy = graph.instantiate(policy_id);
    let sleeve = graph.instantiate(sleeve_id);
    let hooks = graph
        .alias_instance_export(policy, "sleeve:policy/hooks@0.1.0")
        .map_err(message)?;
    graph
        .set_instantiation_argument(sleeve, "sleeve:policy/hooks@0.1.0", hooks)
        .map_err(message)?;
    for name in exports {
        let export = graph
            .alias_instance_export(sleeve, &name)
            .map_err(message)?;
        graph.export(export, &name).map_err(message)?;
    }
    graph.encode(EncodeOptions::default()).map_err(message)
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

#[cfg(test)]
mod tests {
    use super::compose_policy;

    #[tokio::test]
    async fn external_trace_policy_matches_the_compiled_policy() {
        let sleeve = std::fs::read(guest_build::external_policy_sleeve()).unwrap();
        let policy = std::fs::read(guest_build::trace_policy()).unwrap();
        let sleeve = compose_policy(&sleeve, &policy).unwrap();
        let plugin = std::fs::read(guest_build::note_summary()).unwrap();
        let host = sleeve_host::Host::new(
            [
                ("first".into(), "Bring tea".into()),
                ("second".into(), "Book the room".into()),
            ],
            sleeve_host::sleeve_sha256(&sleeve),
        )
        .unwrap();

        let result = host
            .summarize(&plugin, &sleeve, "daily", "first", "second")
            .await
            .unwrap();

        assert_eq!(result.value, "Bring tea; Book the room");
        assert_eq!(
            result.audit,
            [
                "invocation start daily",
                "call 1 example:notes/notes@0.1.0.read name=first",
                "return 1 ok",
                "call 2 example:notes/notes@0.1.0.read name=second",
                "return 2 ok",
                "invocation end daily returned",
            ]
        );
    }
}
