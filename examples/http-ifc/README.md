# HTTP IFC

This example composes one untrusted plugin with the tracing IFC sleeve. A
loopback server is the allowed HTTP origin. The plugin first reads a secret
note and then tries to fetch, followed by a fresh invocation that fetches
before reading the same note. The printed trace shows the policy decision for
each ordering.

Run it from the repository root:

```console
cargo run -p http-ifc
```
