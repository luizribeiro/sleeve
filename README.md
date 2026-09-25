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

## What this shows

On an Apple M3 Ultra, medians of five release runs give the following comparison.
The HTTP workload makes 500 small requests to a loopback server; filesystem rows
transfer 1 MiB through a relayed stream.

| Evidence | Direct | Host middleware | Sleeve |
|---|---:|---:|---:|
| Note call | 279 ns | 428 ns | 1,827 ns |
| HTTP send | 199.30 µs | 190.18 µs | 205.95 µs |
| File write | 2.42 ms | 2.57 ms | 2.35 ms |
| File read | 2.35 ms | 1.95 ms | 2.54 ms |
| HTTP wrapper lines | — | 296 | 312 |
| Filesystem wrapper lines | — | 655 | 365 |

The sleeve precisely refuses a secret-label raise while an HTTP or file writer
is open, permits fetch-then-read and secret-to-secret writes, enforces the body
cap, and rejects public or unknown file sinks after a secret read. Its platform
contract is eight functions: audit, three settings, two lifecycle calls, and two
anchor calls. A custom compiled-in policy takes 135 lines across policy, sleeve,
and host source; an independently composed trace policy works but adds another
component call, measuring 3,561 ns versus 2,361 ns for compiled-in tracing.

The same buffered HTTP components agree under Wasmtime and jco 1.35. The
invocation-long relay used for filesystem streams does not: jco cannot advance a
second concurrent async export while the first awaits it. This makes sleeves a
useful Wasmtime-side complement to host middleware today, not a portable primary
replacement.

Reproduce the timings with `cargo run --release -p sleeve-benchmark` and the
line counts with `support/count-wrapper-lines.sh`.
