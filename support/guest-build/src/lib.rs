//! Build-time access to WebAssembly components used by tests.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::path::Path;

/// Returns the path to the asynchronous greeting component.
#[must_use]
pub fn smoke() -> &'static Path {
    Path::new(env!("SMOKE_COMPONENT"))
}

#[cfg(test)]
mod tests {
    use std::process::Command;

    use wasmtime::component::{Component, Linker, ResourceTable};
    use wasmtime::{Config, Engine, Result, Store};
    use wasmtime_wasi::{WasiCtx, WasiCtxBuilder, WasiCtxView, WasiView};

    mod bindings {
        wasmtime::component::bindgen!({
            path: "../../guests/plugins/smoke/wit",
            world: "smoke",
            exports: { default: async | store },
            require_store_data_send: true,
        });
    }

    struct State {
        table: ResourceTable,
        wasi: WasiCtx,
    }

    impl State {
        fn new() -> Self {
            Self {
                table: ResourceTable::new(),
                wasi: WasiCtxBuilder::new().build(),
            }
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

    fn engine() -> Result<Engine> {
        let mut config = Config::new();
        config.wasm_component_model_async(true);
        config.concurrency_support(true);
        Engine::new(&config)
    }

    fn linker(engine: &Engine) -> Result<Linker<State>> {
        let mut linker = Linker::new(engine);
        wasmtime_wasi::p2::add_to_linker_async(&mut linker)?;
        Ok(linker)
    }

    #[tokio::test]
    async fn greets_through_the_concurrent_call_api() -> Result<()> {
        let engine = engine()?;
        let linker = linker(&engine)?;
        let component = Component::from_file(&engine, super::smoke())?;
        let mut store = Store::new(&engine, State::new());
        let guest = bindings::Smoke::instantiate_async(&mut store, &component, &linker).await?;

        let greeting = store
            .run_concurrent(async move |accessor| {
                guest
                    .sleeve_smoke_greeter()
                    .call_greet(accessor, "Grace".to_owned())
                    .await
            })
            .await??;

        assert_eq!(greeting, "Hello, Grace!");
        Ok(())
    }

    #[test]
    fn component_wit_lists_the_greeting_export() {
        let output = Command::new("wasm-tools")
            .args(["component", "wit"])
            .arg(super::smoke())
            .output()
            .unwrap();

        assert!(
            output.status.success(),
            "wasm-tools failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains("greet"));
    }
}
