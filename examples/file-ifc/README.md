# File IFC

This example composes one untrusted plugin with the tracing IFC sleeve. The
host exposes `public/` and `secret/` temporary directories as labeled
preopens. The plugin first tries to read a secret file while a public file
writer remains open, then repeats the read after closing the writer. The
printed trace shows the channel lifetime and both policy decisions.

Run it from the repository root:

```console
cargo run -p file-ifc
```
