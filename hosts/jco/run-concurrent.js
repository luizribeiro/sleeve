import { writeFileSync } from "node:fs";
import { pathToFileURL } from "node:url";

const [componentPath] = process.argv.slice(2);
let reported = false;

process.on("uncaughtException", (error) => {
  report({ status: "trapped", error: String(error) });
  process.exit(0);
});

const component = await import(pathToFileURL(componentPath));
try {
  const [value] = await Promise.all([component.run(), component.helper()]);
  report({ status: "returned", value: String(value) });
} catch (error) {
  report({ status: "trapped", error: String(error) });
}

function report(result) {
  if (!reported) {
    writeFileSync(1, `${JSON.stringify({ name: "concurrent", audit: [], ...result })}\n`);
    reported = true;
  }
}
