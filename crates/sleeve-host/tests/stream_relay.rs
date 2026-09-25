//! Exercises guest channel relays owned by an invocation-long asynchronous task.

use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::task::{Context, Poll};
use std::time::Instant;

use wac_graph::{CompositionGraph, EncodeOptions, types::Package};
use wasmtime::component::{
    Accessor, Component, HasData, Linker, Source, StreamConsumer, StreamReader, StreamResult,
};
use wasmtime::{Config, Engine, Store, StoreContextMut};

mod bindings {
    wasmtime::component::bindgen!({
        path: ["../../wit/platform", "../../wit/stream-relay"],
        world: "example:stream-relay/composed@0.1.0",
        imports: { default: async | store },
        exports: { default: async | store },
        require_store_data_send: true,
    });
}

struct State {
    events: Vec<String>,
    bytes: Arc<AtomicUsize>,
}

#[allow(dead_code, reason = "required as the generated host view type")]
struct StateView<'a>(&'a mut State);

struct StateData;

impl HasData for StateData {
    type Data<'a> = StateView<'a>;
}

impl bindings::example::stream_relay::sink::Host for StateView<'_> {}

impl bindings::example::stream_relay::sink::HostWithStore<State> for StateData {
    async fn accept(
        store: &Accessor<State, Self>,
        expected: u32,
        body: StreamReader<u8>,
        mut trailers: wasmtime::component::FutureReader<u8>,
    ) -> u32 {
        let bytes = store.with(|mut access| Arc::clone(&access.data_mut().bytes));
        let target = bytes.load(Ordering::Relaxed) + expected as usize;
        store.with(|mut access| {
            body.pipe(&mut access, CountBytes(bytes)).unwrap();
            trailers.close(&mut access).unwrap();
            access.data_mut().events.push("host-accepted".into());
        });
        while expected > 0
            && store.with(|mut access| access.data_mut().bytes.load(Ordering::Relaxed)) < target
        {
            tokio::task::yield_now().await;
        }
        17
    }

    async fn log(store: &Accessor<State, Self>, event: String) {
        store.with(|mut access| access.data_mut().events.push(event));
    }
}

struct CountBytes(Arc<AtomicUsize>);

impl StreamConsumer<State> for CountBytes {
    type Item = u8;

    fn poll_consume(
        self: Pin<&mut Self>,
        _: &mut Context<'_>,
        store: StoreContextMut<'_, State>,
        source: Source<'_, Self::Item>,
        finish: bool,
    ) -> Poll<wasmtime::Result<StreamResult>> {
        if finish {
            return Poll::Ready(Ok(StreamResult::Cancelled));
        }
        let mut source = source.as_direct(store);
        let count = source.remaining().len();
        source.mark_read(count);
        self.0.fetch_add(count, Ordering::Relaxed);
        Poll::Ready(Ok(StreamResult::Completed))
    }
}

fn engine() -> Engine {
    let mut config = Config::new();
    config.wasm_component_model_async(true);
    config.concurrency_support(true);
    Engine::new(&config).unwrap()
}

fn compose_pair(plugin: &[u8], sleeve: &[u8]) -> Vec<u8> {
    let mut graph = CompositionGraph::new();
    let sleeve = Package::from_bytes("test:sleeve", None, sleeve, graph.types_mut()).unwrap();
    let plugin = Package::from_bytes("test:plugin", None, plugin, graph.types_mut()).unwrap();
    let sleeve = graph.register_package(sleeve).unwrap();
    let plugin = graph.register_package(plugin).unwrap();
    let sleeve = graph.instantiate(sleeve);
    let plugin = graph.instantiate(plugin);
    let relay = graph
        .alias_instance_export(sleeve, "example:stream-relay/relay@0.1.0")
        .unwrap();
    graph
        .set_instantiation_argument(plugin, "example:stream-relay/relay@0.1.0", relay)
        .unwrap();
    let run = graph.alias_instance_export(plugin, "run").unwrap();
    graph.export(run, "run").unwrap();
    for name in [
        "sleeve:platform/lifecycle@0.1.0",
        "sleeve:platform/anchor@0.1.0",
        "drop-future",
    ] {
        let export = graph.alias_instance_export(sleeve, name).unwrap();
        graph.export(export, name).unwrap();
    }
    graph.encode(EncodeOptions::default()).unwrap()
}

async fn instantiate() -> (Store<State>, bindings::Composed) {
    let engine = engine();
    let plugin = std::fs::read(guest_build::stream_relay_plugin()).unwrap();
    let sleeve = std::fs::read(guest_build::stream_relay_sleeve()).unwrap();
    let component = Component::from_binary(&engine, &compose_pair(&plugin, &sleeve)).unwrap();
    let mut linker = Linker::new(&engine);
    bindings::example::stream_relay::sink::add_to_linker::<_, StateData>(&mut linker, |state| {
        StateView(state)
    })
    .unwrap();
    let mut store = Store::new(
        &engine,
        State {
            events: Vec::new(),
            bytes: Arc::new(AtomicUsize::new(0)),
        },
    );
    let guest = bindings::Composed::instantiate_async(&mut store, &component, &linker)
        .await
        .unwrap();
    store
        .run_concurrent(async |accessor| {
            guest
                .sleeve_platform_lifecycle()
                .call_start(accessor, "relay".into())
                .await
        })
        .await
        .unwrap()
        .unwrap();
    (store, guest)
}

async fn run(
    store: &mut Store<State>,
    guest: &bindings::Composed,
    mode: u8,
    size: u32,
) -> (u32, u32) {
    store
        .run_concurrent(async |accessor| {
            let anchored = guest.sleeve_platform_anchor().call_run(accessor);
            let invoked = async {
                let value = guest.call_run(accessor, mode, size).await.unwrap();
                guest
                    .sleeve_platform_anchor()
                    .call_stop(accessor)
                    .await
                    .unwrap();
                value
            };
            let (anchor, value) = tokio::join!(anchored, invoked);
            (value, anchor.unwrap())
        })
        .await
        .unwrap()
}

async fn end(store: &mut Store<State>, guest: &bindings::Composed) {
    store
        .run_concurrent(async |accessor| {
            guest
                .sleeve_platform_lifecycle()
                .call_end(accessor, "relay".into(), false)
                .await
        })
        .await
        .unwrap()
        .unwrap();
}

fn event_index(events: &[String], name: &str) -> usize {
    events.iter().position(|event| event == name).unwrap()
}

#[tokio::test]
async fn anchor_relays_bytes_and_closes_after_send_returns() {
    let (mut store, guest) = instantiate().await;
    assert_eq!(run(&mut store, &guest, 0, 4).await, (17, 0));
    assert_eq!(store.data().bytes.load(Ordering::Relaxed), 4);
    let events = &store.data().events;
    let returned = event_index(events, "send-returned");
    assert!(returned < event_index(events, "body-closed-state-ok"));
    assert!(returned < event_index(events, "trailers-closed-state-ok"));
    end(&mut store, &guest).await;
}

#[tokio::test]
async fn query_allows_a_read_after_the_plugin_closes_both_channels() {
    let (mut store, guest) = instantiate().await;
    assert_eq!(run(&mut store, &guest, 0, 4).await, (17, 0));
    let events = &store.data().events;
    let closed = event_index(events, "query-closed");
    assert!(event_index(events, "body-closed-state-ok") < closed);
    assert!(event_index(events, "trailers-closed-state-ok") < closed);
    end(&mut store, &guest).await;
}

#[tokio::test]
async fn query_is_refused_while_the_plugin_writer_remains_open() {
    let (mut store, guest) = instantiate().await;
    assert_eq!(run(&mut store, &guest, 2, 4).await, (18, 1));
    assert!(
        store
            .data()
            .events
            .iter()
            .any(|event| event == "query-open")
    );
    end(&mut store, &guest).await;
}

#[tokio::test]
async fn ending_with_a_pending_relay_cancels_it_before_policy_state_ends() {
    let (mut store, guest) = instantiate().await;
    assert_eq!(run(&mut store, &guest, 2, 4).await, (18, 1));
    assert_eq!(store.data().bytes.load(Ordering::Relaxed), 0);
    assert!(
        store
            .data()
            .events
            .iter()
            .any(|event| event == "body-cancelled-closed-state-ok")
    );
    assert!(
        store
            .data()
            .events
            .iter()
            .any(|event| event == "trailers-cancelled-closed-state-ok")
    );
    end(&mut store, &guest).await;
    assert_eq!(store.data().bytes.load(Ordering::Relaxed), 0);
    assert!(
        store
            .data()
            .events
            .iter()
            .all(|event| !event.ends_with("state-error"))
    );
}

#[tokio::test]
#[ignore = "the async import cancels spawned relays when its return value is delivered"]
async fn relay_outlives_the_wrapped_call() {
    let (mut store, guest) = instantiate().await;
    assert_eq!(run(&mut store, &guest, 3, 4).await, (17, 0));
    assert_eq!(store.data().bytes.load(Ordering::Relaxed), 4);
    assert!(
        store
            .data()
            .events
            .iter()
            .any(|event| event == "body-closed-state-ok")
    );
    assert!(
        store
            .data()
            .events
            .iter()
            .any(|event| event == "trailers-closed-state-ok")
    );
}

#[tokio::test]
async fn sync_lifted_future_writer_drop_traps_without_a_task_context() {
    let (mut store, guest) = instantiate().await;
    assert!(
        store
            .run_concurrent(async |accessor| guest.call_drop_future(accessor).await)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn compares_anchor_relay_and_buffered_throughput_for_one_mibibyte() {
    const SIZE: u32 = 1024 * 1024;
    let (mut store, guest) = instantiate().await;

    let start = Instant::now();
    assert_eq!(run(&mut store, &guest, 0, SIZE).await, (17, 0));
    let relayed = start.elapsed();
    end(&mut store, &guest).await;

    let (mut store, guest) = instantiate().await;
    let start = Instant::now();
    assert_eq!(run(&mut store, &guest, 1, SIZE).await, (17, 0));
    let buffered = start.elapsed();
    end(&mut store, &guest).await;

    assert_eq!(store.data().bytes.load(Ordering::Relaxed), SIZE as usize);
    eprintln!("1 MiB anchor={relayed:?} buffered={buffered:?}");
}
