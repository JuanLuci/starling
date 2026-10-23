// Builds Starling's shared JavaScript engine (engine.js, the starling-js world) with
// componentize-js (componentize.mjs) into target/js-engine/engine.wasm, which the game embeds
// (horizon-mods' build.rs), keeping this computer out of it. Version 0.23 or later: before it,
// every string a mod passed to the game (draw.text, keys.is-control-down...) leaked its memory,
// and a mod drawing text each frame ran out in minutes. The JavaScript engine's
// snapshot keeps the environment its pre-initializer ran in and the paths of the files it loaded,
// so the build runs with an almost empty environment and a temporary folder that names no one,
// and the result is refused if it still holds the home folder or the user name.
import { spawnSync } from "node:child_process";
import { mkdirSync, readFileSync, rmSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { homedir, platform, userInfo } from "node:os";

const OUT = "../../../target/js-engine/engine.wasm";
// A path beside this script.
const here = (file) => resolve(dirname(fileURLToPath(import.meta.url)), file);
const windows = platform() === "win32";
const temp = windows ? join(process.env.SystemDrive || "C:", "\\hmod-build") : "/tmp/hmod-build";
mkdirSync(temp, { recursive: true });
mkdirSync(dirname(here(OUT)), { recursive: true });

// On Windows libuv gives a child the parent's "required" variables it lacks (USERNAME,
// USERPROFILE, HOMEPATH, PATH...), so they are all set here, to values that name no one. The
// pre-initializer runs through the shell, hence ComSpec.
const env = { TEMP: temp, TMP: temp, TMPDIR: temp, PATH: dirname(process.execPath) };
if (windows) {
  for (const name of ["SystemRoot", "SystemDrive", "ComSpec", "windir"]) {
    env[name] = process.env[name];
  }
  Object.assign(env, {
    HOMEDRIVE: process.env.SystemDrive || "C:",
    HOMEPATH: "\\hmod-build",
    USERPROFILE: temp,
    USERNAME: "builder",
    USERDOMAIN: "builder",
    LOGONSERVER: "",
  });
  // The pre-initializer looks for its settings in the profile's AppData, which Windows finds
  // through USERPROFILE.
  for (const dir of ["Roaming", "Local"]) {
    mkdirSync(join(temp, "AppData", dir), { recursive: true });
  }
}
// Run from a neutral folder (Windows keeps the current folder in a variable too), with the
// files by full path. Not the temporary folder itself: componentize-js takes its work folder,
// made there, for one under the current folder and loses the source.
const cwd = join(temp, "cwd");
mkdirSync(cwd, { recursive: true });
const built = spawnSync(
  process.execPath,
  [
    here("componentize.mjs"), here("engine.js"), here("../wit/starling.wit"), here(OUT),
  ],
  { env, cwd, stdio: "inherit" },
);
if (built.status !== 0) {
  process.exit(built.status ?? 1);
}

// What would tell who built it: the home folder (either slash) and the user name as a variable
// (alone, a short name would be found in any text).
const wasm = readFileSync(here(OUT));
const home = homedir();
const user = userInfo().username;
const leaks = [home, home.replaceAll("\\", "/"), `USERNAME=${user}`, `USER=${user}`]
  .filter((s, i, all) => all.indexOf(s) === i)
  .filter((s) => wasm.includes(Buffer.from(s)));
if (leaks.length > 0) {
  rmSync(here(OUT));
  console.error(`the engine held ${leaks.join(", ")}: not kept (build from a folder outside your home)`);
  process.exit(1);
}
console.log(`${here(OUT)}: nothing of this computer in it`);
