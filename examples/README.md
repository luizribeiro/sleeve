# Examples

- [`trace`](trace/) composes a note-reading plugin with an immediate audit
  policy and prints the resulting call trace.
- [`http-ifc`](http-ifc/) shows how call ordering and open request channels
  affect HTTP decisions under the tracing information-flow policy.
- [`file-ifc`](file-ifc/) shows how labeled preopens and file-writer lifetimes
  affect filesystem decisions under the tracing information-flow policy.
- [`jco`](jco/) runs the HTTP ordering scenarios through the same composed
  component under Node.
- [`custom-policy`](custom-policy/) builds a new policy and sleeve variant
  inside the example, then refuses a plugin's fourth HTTP request.
