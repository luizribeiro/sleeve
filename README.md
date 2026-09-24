# sleeve

`sleeve` is an experiment in WebAssembly middleware by component composition.
A sleeve component sits in front of an untrusted plugin, turns intercepted calls
into policy-neutral events, runs a compiled-in policy chain, and forwards allowed
operations to the host.

The host composes and verifies the graph at load time:

```rust,ignore
let approved_digest = sleeve_host::sleeve_sha256(approved_sleeve);
// Later, reject any candidate whose bytes do not match that independent pin.
let composed = sleeve_host::compose(plugin, candidate_sleeve, approved_digest)?;
```

The [`trace` example](examples/trace/) reads two notes through a tracing sleeve.
Its audit log makes the extra boundary visible:

```text
invocation start note-summary
call 1 example:notes/notes@0.1.0.read name=today
return 1 ok
call 2 example:notes/notes@0.1.0.read name=project
return 2 ok
invocation end note-summary returned
result: Call Ada at 10; The launch is Friday
```

Run it with `cargo run -p trace` inside the Nix development shell.
