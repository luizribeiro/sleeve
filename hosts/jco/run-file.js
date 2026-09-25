import { _setPreopens } from "@bytecodealliance/preview3-shim/filesystem";

import { invoke } from "./invoke.js";
import { configure } from "./platform.js";

const [componentPath, scenarioText, invocation, publicPath, secretPath] =
  process.argv.slice(2);

configure({
  notes: {},
  bodyLimit: 0,
  origins: [],
  preopenLabels: { public: "public", secret: "secret" },
});
_setPreopens({ public: publicPath, secret: secretPath });

await invoke(componentPath, invocation, (component) =>
  component.run(Number(scenarioText)),
);
