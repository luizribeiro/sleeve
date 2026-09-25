# jco HTTP IFC

This example composes the HTTP scenario plugin with the tracing IFC sleeve,
transpiles the verified composition with jco, and runs it under Node 24. It
prints the same two ordering decisions as `http-ifc`, including the audit
events retained before a denial traps the plugin.

Install the locked Node dependencies and run from the repository root:

```console
npm ci --prefix hosts/jco --ignore-scripts
cargo run -p jco
```
