import { spawnSync } from "node:child_process";
import {
  mkdir,
  mkdtemp,
  readFile,
  rm,
  symlink,
  writeFile,
} from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";

const [component, scenariosPath] = process.argv.slice(2);
const scenarios = JSON.parse(await readFile(scenariosPath, "utf8"));
const root = await mkdtemp(path.join(tmpdir(), "sleeve-jco-"));
const publicPath = path.join(root, "public");
const secretPath = path.join(root, "secret");
await mkdir(publicPath);
await mkdir(secretPath);
await writeFile(path.join(secretPath, "note.txt"), "classified");
await writeFile(path.join(publicPath, "existing.txt"), "public");
await symlink("../secret/note.txt", path.join(publicPath, "secret-link"));

try {
  const results = scenarios.map((scenario) => {
    const run = spawnSync(
      process.execPath,
      [
        path.join(import.meta.dirname, "run-file.js"),
        path.resolve(component),
        scenario.input,
        scenario.name,
        publicPath,
        secretPath,
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
} finally {
  await rm(root, { force: true, recursive: true });
}
