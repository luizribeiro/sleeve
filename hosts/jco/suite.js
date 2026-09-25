import { spawnSync } from "node:child_process";
import { readFile } from "node:fs/promises";
import path from "node:path";

const [component, scenariosPath, allowed, blocked, uppercaseAllowed, origin, selected] =
  process.argv.slice(2);
const scenarios = JSON.parse(await readFile(scenariosPath, "utf8"));
const authorities = { allowed, blocked, "uppercase-allowed": uppercaseAllowed };
const results = scenarios
  .filter((scenario) => !selected || scenario.name === selected)
  .map((scenario) => {
    const run = spawnSync(
      process.execPath,
      [
        path.join(import.meta.dirname, "run.js"),
        path.resolve(component),
        scenario.input,
        authorities[scenario.authority],
        scenario.body_size,
        scenario.name,
        origin,
      ].map(String),
      { encoding: "utf8" },
    );
    if (run.error) {
      throw run.error;
    }
    if (run.status !== 0) {
      throw new Error(
        `${scenario.name}: ${run.stderr || `exited with status ${run.status}`}`,
      );
    }
    return { name: scenario.name, ...JSON.parse(run.stdout) };
  });

process.stdout.write(`${JSON.stringify(results)}\n`);
