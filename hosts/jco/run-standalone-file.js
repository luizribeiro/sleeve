import { _setPreopens } from "@bytecodealliance/preview3-shim/filesystem";
import { writeFileSync } from "node:fs";
import { pathToFileURL } from "node:url";

const [componentPath, exportName, publicPath, secretPath] = process.argv.slice(2);
let reported = false;

process.on("uncaughtException", (error) => {
  report({ status: "trapped", error: String(error) });
  process.exit(0);
});

_setPreopens({ public: publicPath, secret: secretPath });
const component = await import(pathToFileURL(componentPath));
try {
  const value = await component[exportName]();
  report({ status: "returned", value });
} catch (error) {
  report({ status: "trapped", error: String(error) });
}

function report(result) {
  if (!reported) {
    writeFileSync(
      1,
      `${JSON.stringify({ name: exportName, audit: [], ...result })}\n`,
    );
    reported = true;
  }
}
