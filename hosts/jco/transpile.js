import { spawnSync } from "node:child_process";
import path from "node:path";

const [componentPath, outputDirectory] = process.argv.slice(2);
const component = path.resolve(componentPath);
const output = path.resolve(outputDirectory);
const platform = relativeImport(
  output,
  path.join(import.meta.dirname, "platform.js"),
);
const result = spawnSync(
  "npx",
  [
    "--no-install",
    "jco",
    "transpile",
    component,
    "--out-dir",
    output,
    "--name",
    "component",
    "--async-mode",
    "jspi",
    "--async-wasi-imports",
    "--async-wasi-exports",
    "--no-typescript",
    "--map",
    `example:notes/notes=${platform}`,
    "--map",
    `sleeve:platform/audit=${platform}`,
    "--map",
    `sleeve:platform/settings=${platform}`,
  ],
  { cwd: import.meta.dirname, stdio: "inherit" },
);

if (result.error) {
  throw result.error;
}
process.exitCode = result.status;

function relativeImport(directory, file) {
  let relative = path
    .relative(path.resolve(directory), file)
    .replaceAll(path.sep, "/");
  if (!relative.startsWith(".")) {
    relative = `./${relative}`;
  }
  return relative;
}
