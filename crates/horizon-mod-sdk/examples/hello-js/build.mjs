// Builds mod.wasm with componentize-js, keeping this computer out of it. The JavaScript engine's
// snapshot keeps the environment its pre-initializer ran in and the paths of the files it loaded,
// so the build runs with an almost empty environment and a temporary folder that names no one,
// and the result is refused if it still holds the home folder or the user name.
import { spawnSync } from "node:child_process";
import { mkdirSync, readFileSync, rmSync } from "node:fs";
import { createRequire } from "node:module";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { homedir, platform, userInfo } from "node:os";

const OUT = "mod.wasm";
const windows = platform() === "win32";
const temp = windows ? join(process.env.SystemDrive || "C:", "\\hmod-build") : "/tmp/hmod-build";
mkdirSync(temp, { recursive: true });

// jco's command line, beside its main module.
const jco = join(dirname(createRequire(import.meta.url).resolve("@bytecodealliance/jco")), "jco.js");
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
// Run from the neutral folder (Windows keeps the current folder in a variable too), with this
// mod's files by full path.
const here = (file) => resolve(dirname(fileURLToPath(import.meta.url)), file);
const built = spawnSync(
  process.execPath,
  [
    jco, "componentize", here("src/mod.js"), "--wit", here("../../wit"),
    "--world-name", "horizon-mod", "--out", here(OUT), "--disable", "all",
  ],
  { env, cwd: temp, stdio: "inherit" },
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
  console.error(`mod.wasm held ${leaks.join(", ")}: not kept (build from a folder outside your home)`);
  process.exit(1);
}
console.log(`${OUT}: nothing of this computer in it`);
