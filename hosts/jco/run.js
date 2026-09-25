import { invoke } from "./invoke.js";
import { configure } from "./platform.js";

const [componentPath, scenarioText, authority, bodySizeText, invocation, origin] =
  process.argv.slice(2);

configure({
  notes: { secret: "classified" },
  bodyLimit: 8,
  origins: [origin],
});

await invoke(componentPath, invocation, (component) =>
  component.run(
    Number(scenarioText),
    authority,
    Number(bodySizeText),
  ),
);
