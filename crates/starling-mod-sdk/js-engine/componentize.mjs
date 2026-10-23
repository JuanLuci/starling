// Run by build.mjs, in its neutral environment: engine.js and the WIT in, the component out.
import { componentize } from "@bytecodealliance/componentize-js";
import { readFileSync, writeFileSync } from "node:fs";

const [source, wit, out] = process.argv.slice(2);
// The WIT goes in as text: componentize-js 0.23 cannot open a Windows path to it.
const { component } = await componentize(readFileSync(source, "utf8"), {
  witWorld: readFileSync(wit, "utf8"),
  worldName: "starling-js",
  // No WASI at all: the engine reaches the game only through starling:mods.
  disableFeatures: ["stdio", "random", "clocks", "http", "fetch-event"],
});
writeFileSync(out, component);
console.log(`OK ${out}`);
