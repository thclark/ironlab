// Check that a browser bundle initialises: load its wasm module through its own JavaScript bindings and ask the module
// for its version.
//
// Usage:
//
//     node scripts/check-web-bundle.mjs DIR
//
// DIR is a directory that scripts/build-web.sh assembled, which runs this check on every bundle it builds. The check
// exists because an optimiser can break a module that the unoptimised tests pass: binaryen 108 rewrote the export
// through which the bindings reach the module's table of JavaScript values, and every figure on ironlab.org failed to
// start. Initialising the module touches no browser interface, so Node runs the check. Drawing a figure needs a
// graphics device, which Node does not have, so this check does not replace the tests in headless Chrome; those run
// the unoptimised module, and this runs the one that ships.
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";
import { pathToFileURL } from "node:url";

const dir = process.argv[2];
if (!dir) {
  console.error("usage: node scripts/check-web-bundle.mjs DIR");
  process.exit(2);
}

try {
  const bindings = await import(pathToFileURL(resolve(dir, "ironlab_core.js")).href);
  const bytes = await readFile(resolve(dir, "ironlab_core_bg.wasm"));
  await bindings.default({ module_or_path: bytes });
  console.log(`The bundle in ${dir} initialises (ironlab-web ${bindings.Session.version()}).`);
} catch (error) {
  console.error(`error: the bundle in ${dir} does not initialise: ${error?.stack ?? error}`);
  process.exit(1);
}
