use std::collections::BTreeMap;

use wasmtime::component::{Accessor, Component, HasData, Linker};
use wasmtime::{Config, Engine, Store};

use crate::compose;

mod bindings {
    wasmtime::component::bindgen!({
        path: ["../../wit/notes", "../../wit/platform", "wit"],
        world: "sleeve:host/composed@0.1.0",
        imports: {
            "example:notes/notes@0.1.0": async | store,
            default: trappable,
        },
        exports: { default: async | store },
        require_store_data_send: true,
    });
}

/// Owns immutable host services used to create one store per invocation.
pub struct Host {
    engine: Engine,
    notes: BTreeMap<String, String>,
    sleeve_sha256: [u8; 32],
}

/// The plugin's return value and the audit records persisted during its run.
#[derive(Debug, Eq, PartialEq)]
pub struct InvocationResult {
    /// Value returned by the plugin export.
    pub value: String,
    /// Records in the order the platform received them.
    pub audit: Vec<String>,
}

/// A completed or trapped export together with audit records already persisted.
#[derive(Debug, Eq, PartialEq)]
pub struct InvocationAttempt {
    /// The plugin value or trap diagnostic.
    pub value: Result<String, String>,
    /// Records in the order the platform received them.
    pub audit: Vec<String>,
}

struct State {
    notes: BTreeMap<String, String>,
    audit: Vec<String>,
}

struct StateView<'a>(&'a mut State);

struct StateData;

impl HasData for StateData {
    type Data<'a> = StateView<'a>;
}

impl bindings::example::notes::notes::Host for StateView<'_> {}

impl bindings::example::notes::notes::HostWithStore<State> for StateData {
    #[allow(clippy::unused_async_trait_impl)]
    async fn read(store: &Accessor<State, Self>, name: String) -> String {
        store.with(|mut access| {
            access
                .data_mut()
                .notes
                .get(&name)
                .cloned()
                .unwrap_or_else(|| format!("missing note: {name}"))
        })
    }
}

impl bindings::sleeve::platform::audit::Host for StateView<'_> {
    fn log(&mut self, event: String) -> wasmtime::Result<()> {
        self.0.audit.push(event);
        Ok(())
    }
}

impl Host {
    /// Creates a host with an in-memory notes service and an approved sleeve pin.
    ///
    /// ```no_run
    /// # fn approved_sleeve() -> Vec<u8> { Vec::new() }
    /// let sleeve = approved_sleeve();
    /// let pin = sleeve_host::sleeve_sha256(&sleeve);
    /// let host = sleeve_host::Host::new([], pin)
    ///     .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    /// # let _ = host;
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    ///
    /// # Errors
    ///
    /// Returns an engine configuration error when async components are unavailable.
    pub fn new(
        notes: impl IntoIterator<Item = (String, String)>,
        sleeve_sha256: [u8; 32],
    ) -> wasmtime::Result<Self> {
        let mut config = Config::new();
        config.wasm_component_model_async(true);
        config.concurrency_support(true);
        Ok(Self {
            engine: Engine::new(&config)?,
            notes: notes.into_iter().collect(),
            sleeve_sha256,
        })
    }

    /// Composes and runs `summarize` in a fresh component instance and store.
    ///
    /// # Errors
    ///
    /// Returns a load refusal, instantiation error, host error, or plugin trap.
    pub async fn summarize(
        &self,
        plugin: &[u8],
        sleeve: &[u8],
        invocation: &str,
        first: &str,
        second: &str,
    ) -> anyhow::Result<InvocationResult> {
        let attempt = self
            .summarize_with_audit(plugin, sleeve, invocation, first, second)
            .await?;
        let value = attempt.value.map_err(anyhow::Error::msg)?;
        Ok(InvocationResult {
            value,
            audit: attempt.audit,
        })
    }

    /// Runs `summarize` while retaining audit records when the plugin traps.
    ///
    /// # Errors
    ///
    /// Returns a load, composition, instantiation, or lifecycle error. A trap
    /// from the plugin export is returned in [`InvocationAttempt::value`].
    pub async fn summarize_with_audit(
        &self,
        plugin: &[u8],
        sleeve: &[u8],
        invocation: &str,
        first: &str,
        second: &str,
    ) -> anyhow::Result<InvocationAttempt> {
        let bytes = compose(plugin, sleeve, self.sleeve_sha256)?;
        let component = Component::from_binary(&self.engine, &bytes).map_err(anyhow_message)?;
        let mut linker = Linker::new(&self.engine);
        bindings::example::notes::notes::add_to_linker::<_, StateData>(&mut linker, |state| {
            StateView(state)
        })
        .map_err(anyhow_message)?;
        bindings::sleeve::platform::audit::add_to_linker::<_, StateData>(&mut linker, |state| {
            StateView(state)
        })
        .map_err(anyhow_message)?;
        let mut store = Store::new(
            &self.engine,
            State {
                notes: self.notes.clone(),
                audit: Vec::new(),
            },
        );
        let guest = bindings::Composed::instantiate_async(&mut store, &component, &linker)
            .await
            .map_err(anyhow_message)?;
        store
            .run_concurrent(async |accessor| {
                guest
                    .sleeve_platform_lifecycle()
                    .call_start(accessor, invocation.to_owned())
                    .await
            })
            .await
            .map_err(anyhow_message)?
            .map_err(anyhow_message)?;
        let value = store
            .run_concurrent(async |accessor| {
                guest
                    .call_summarize(accessor, first.to_owned(), second.to_owned())
                    .await
            })
            .await;
        let value = match value {
            Ok(Ok(value)) => {
                store
                    .run_concurrent(async |accessor| {
                        guest
                            .sleeve_platform_lifecycle()
                            .call_end(accessor, invocation.to_owned(), false)
                            .await
                    })
                    .await
                    .map_err(anyhow_message)?
                    .map_err(anyhow_message)?;
                Ok(value)
            }
            Ok(Err(error)) | Err(error) => Err(error.to_string()),
        };
        Ok(InvocationAttempt {
            value,
            audit: store.into_data().audit,
        })
    }
}

fn anyhow_message(error: impl std::fmt::Display) -> anyhow::Error {
    anyhow::anyhow!(error.to_string())
}
