# Trace

This example composes an untrusted note-summary plugin with a tracing sleeve at
load time. The plugin's two `notes.read` imports are wired to the sleeve, the
sleeve records each call before forwarding it to the host, and the host pins
the approved sleeve digest before loading it. It prints the persisted audit
records before the plugin's result.

Run it from the repository root:

```console
cargo run -p trace
```
