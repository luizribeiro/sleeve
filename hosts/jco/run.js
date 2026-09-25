import { writeFileSync } from "node:fs";
import { pathToFileURL } from "node:url";

import { auditRecords, configure } from "./platform.js";

const [componentPath, scenarioText, authority, bodySizeText, invocation, origin] =
  process.argv.slice(2);

configure({
  notes: { secret: "classified" },
  bodyLimit: 8,
  origins: [origin],
});

let reported = false;
process.on("uncaughtException", (error) => {
  report({ status: "trapped", error: String(error) });
  process.exit(0);
});

const component = await import(pathToFileURL(componentPath));
component.lifecycle.start(invocation);
const anchor = component.anchor.run();
let outcome;
try {
  const value = await component.run(
    Number(scenarioText),
    authority,
    Number(bodySizeText),
  );
  await component.anchor.stop();
  await anchor;
  component.lifecycle.end(invocation, false);
  outcome = { status: "returned", value };
} catch (error) {
  outcome = { status: "trapped", error: String(error) };
}

report(outcome);

function report(result) {
  if (!reported) {
    writeFileSync(1, `${JSON.stringify({ ...result, audit: auditRecords() })}\n`);
    reported = true;
  }
}
