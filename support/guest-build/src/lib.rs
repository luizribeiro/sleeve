//! Build-time access to WebAssembly components used by tests.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::path::Path;

/// Returns the path to the plugin with a lookalike notes import.
#[must_use]
pub fn bypass() -> &'static Path {
    Path::new(env!("BYPASS_COMPONENT"))
}

/// Returns the path to the sleeve whose policy counts calls.
#[must_use]
pub fn counting_sleeve() -> &'static Path {
    Path::new(env!("COUNTING_SLEEVE_COMPONENT"))
}

/// Returns the path to the sleeve whose inner policy denies calls.
#[must_use]
pub fn deny_sleeve() -> &'static Path {
    Path::new(env!("DENY_SLEEVE_COMPONENT"))
}

/// Returns the path to the plugin with an unrelated direct import.
#[must_use]
pub fn direct_import() -> &'static Path {
    Path::new(env!("DIRECT_IMPORT_COMPONENT"))
}

/// Returns the path to the plugin that imports an unwrapped filesystem method.
#[must_use]
pub fn filesystem_bypass() -> &'static Path {
    Path::new(env!("FILESYSTEM_BYPASS_COMPONENT"))
}

/// Returns the plugin generated from Wasmtime's upstream p3 filesystem WIT.
#[must_use]
pub fn filesystem_upstream() -> &'static Path {
    Path::new(env!("FILESYSTEM_UPSTREAM_COMPONENT"))
}

/// Returns the path to the plugin containing the shared filesystem scenarios.
#[must_use]
pub fn file_scenarios() -> &'static Path {
    Path::new(env!("FILE_SCENARIOS_COMPONENT"))
}

/// Returns the path to the filesystem sleeve with IFC enabled.
#[must_use]
pub fn file_ifc_sleeve() -> &'static Path {
    Path::new(env!("FILE_IFC_SLEEVE_COMPONENT"))
}

/// Returns the path to the plugin that imports an unwrapped HTTP interface.
#[must_use]
pub fn http_bypass() -> &'static Path {
    Path::new(env!("HTTP_BYPASS_COMPONENT"))
}

/// Returns the path to the plugin containing the shared HTTP scenarios.
#[must_use]
pub fn http_scenarios() -> &'static Path {
    Path::new(env!("HTTP_SCENARIOS_COMPONENT"))
}

/// Returns the path to the plugin that reads two notes.
#[must_use]
pub fn note_summary() -> &'static Path {
    Path::new(env!("NOTE_SUMMARY_COMPONENT"))
}

/// Returns the path to the notes sleeve with an empty policy chain.
#[must_use]
pub fn passthrough_sleeve() -> &'static Path {
    Path::new(env!("PASSTHROUGH_SLEEVE_COMPONENT"))
}

/// Returns the path to the plugin that exports a host-facing platform interface.
#[must_use]
pub fn platform_export() -> &'static Path {
    Path::new(env!("PLATFORM_EXPORT_COMPONENT"))
}

/// Returns the path to the plugin that imports a host-facing platform interface.
#[must_use]
pub fn platform_import() -> &'static Path {
    Path::new(env!("PLATFORM_IMPORT_COMPONENT"))
}

/// Returns the path to the plugin that repeatedly reads one note.
#[must_use]
pub fn read_many() -> &'static Path {
    Path::new(env!("READ_MANY_COMPONENT"))
}

/// Returns the plugin that writes to relayed channels.
#[must_use]
pub fn stream_relay_plugin() -> &'static Path {
    Path::new(env!("STREAM_RELAY_PLUGIN_COMPONENT"))
}

/// Returns the sleeve that relays channels to a host sink.
#[must_use]
pub fn stream_relay_sleeve() -> &'static Path {
    Path::new(env!("STREAM_RELAY_SLEEVE_COMPONENT"))
}

/// Returns the path to the notes sleeve with tracing enabled.
#[must_use]
pub fn trace_sleeve() -> &'static Path {
    Path::new(env!("TRACE_SLEEVE_COMPONENT"))
}

/// Returns the path to the notes and HTTP sleeve with IFC enabled.
#[must_use]
pub fn ifc_sleeve() -> &'static Path {
    Path::new(env!("IFC_SLEEVE_COMPONENT"))
}

/// Returns the path to the notes and HTTP sleeve with tracing and IFC enabled.
#[must_use]
pub fn trace_ifc_sleeve() -> &'static Path {
    Path::new(env!("TRACE_IFC_SLEEVE_COMPONENT"))
}

/// Returns the path to the filesystem sleeve with tracing and IFC enabled.
#[must_use]
pub fn trace_file_ifc_sleeve() -> &'static Path {
    Path::new(env!("TRACE_FILE_IFC_SLEEVE_COMPONENT"))
}

/// Returns the path to the plugin that traps after reading a note.
#[must_use]
pub fn trap_after_read() -> &'static Path {
    Path::new(env!("TRAP_AFTER_READ_COMPONENT"))
}

#[cfg(test)]
fn wasip2_note_summary() -> &'static Path {
    Path::new(env!("WASIP2_NOTE_SUMMARY_COMPONENT"))
}

#[cfg(test)]
fn wasip2_passthrough_sleeve() -> &'static Path {
    Path::new(env!("WASIP2_PASSTHROUGH_SLEEVE_COMPONENT"))
}

/// Returns the path to the asynchronous greeting component.
#[must_use]
pub fn smoke() -> &'static Path {
    Path::new(env!("SMOKE_COMPONENT"))
}

#[cfg(test)]
mod tests {
    use std::path::Path;
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

    #[test]
    fn direct_import_export_world_builds_for_wasip2() {
        let wit = component_wit(super::wasip2_passthrough_sleeve());
        assert!(wit.contains("import example:notes/notes@0.1.0;"));
        assert!(wit.contains("export example:notes/notes@0.1.0;"));
    }

    #[test]
    fn unknown_unknown_componentization_removes_wasip2_adapter_imports() {
        let wasip2_wit = component_wit(super::wasip2_note_summary());
        let wasip2 = imports(&wasip2_wit);
        assert_eq!(
            wasip2,
            [
                "example:notes/notes@0.1.0",
                "wasi:io/poll@0.2.9",
                "wasi:clocks/monotonic-clock@0.2.9",
                "wasi:io/error@0.2.9",
                "wasi:io/streams@0.2.9",
                "wasi:cli/stdout@0.2.9",
                "wasi:cli/stderr@0.2.9",
                "wasi:cli/stdin@0.2.9",
                "wasi:cli/environment@0.2.9",
                "wasi:cli/exit@0.2.9",
                "wasi:cli/terminal-input@0.2.9",
                "wasi:cli/terminal-output@0.2.9",
                "wasi:cli/terminal-stdin@0.2.9",
                "wasi:cli/terminal-stdout@0.2.9",
                "wasi:cli/terminal-stderr@0.2.9",
            ]
        );
        assert_eq!(
            imports(&component_wit(super::note_summary())),
            ["example:notes/notes@0.1.0"]
        );
    }

    fn component_wit(path: &Path) -> String {
        let output = Command::new("wasm-tools")
            .args(["component", "wit"])
            .arg(path)
            .output()
            .unwrap();
        assert!(output.status.success());
        String::from_utf8(output.stdout).unwrap()
    }

    fn imports(wit: &str) -> Vec<&str> {
        wit.lines()
            .map(str::trim)
            .filter_map(|line| line.strip_prefix("import "))
            .map(|line| line.trim_end_matches(';'))
            .collect()
    }
}
