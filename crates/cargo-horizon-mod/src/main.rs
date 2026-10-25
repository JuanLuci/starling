//! `cargo horizon-mod`: make Horizon mods in Rust (`docs/mods.md`).
//!
//! - `new <path> [--name "Name"] [--sdk-path <dir>]`: a mod crate from the template.
//! - `build [--debug]`: the `.hmod` in `target/horizon-mod/`, with the crate's `assets/` and
//!   `data/` folders (the mod's own sprites: `assets/<id>/textures/gui/sprites/`).
//! - `run [--game <horizon-client>] [-- <game args>]`: build, then play with
//!   the mod; run `build` again and the game reloads it.
//! - `push [--debug] [--to <folder>]`: build, then copy the `.hmod` to the Starling on the phone
//!   adb reaches (`ANDROID_SERIAL` picks one of several); the game reloads it within a second.
use horizon_mods::{
    Manifest, Package,
    manifest::{MANIFEST, component_imports, componentize},
};
use std::{
    path::{Path, PathBuf},
    process::{Command, ExitCode, Stdio},
};

const TARGET: &str = "wasm32-unknown-unknown";
/// Starling's SDK, from its repository (`--sdk-path` for a local one).
const SDK_GIT: &str = "{ git = \"https://github.com/JuanLuci/starling\", branch = \"android\" }";

fn main() -> ExitCode {
    // Cargo runs `cargo-horizon-mod horizon-mod <args>`.
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|a| a == "horizon-mod") {
        args.remove(0);
    }
    let result = match args.first().map(String::as_str) {
        Some("new") => new(&args[1..]),
        Some("build") => build(!args.iter().any(|a| a == "--debug")).map(|p| {
            println!("Built {}", p.display());
        }),
        Some("run") => run(&args[1..]),
        Some("push") => push(&args[1..]),
        _ => Err("usage: cargo horizon-mod new <path> | build [--debug] | run [--game <horizon-client>] [-- <game args>] | push [--debug] [--to <folder>]".into()),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

/// The value after `flag` in `args`.
fn flag<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    let i = args.iter().position(|a| a == flag)?;
    args.get(i + 1).map(String::as_str)
}

fn new(args: &[String]) -> Result<(), String> {
    let path = args
        .first()
        .filter(|a| !a.starts_with("--"))
        .ok_or("cargo horizon-mod new <path>")?;
    let dir = Path::new(path);
    if dir.exists() {
        return Err(format!("{} already exists", dir.display()));
    }
    let base = dir
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or("the path needs a folder name")?;
    let id: String = base
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect();
    let name = flag(args, "--name").map_or_else(|| base.replace(['-', '_'], " "), str::to_string);
    let sdk = match flag(args, "--sdk-path") {
        Some(p) => {
            let p = std::fs::canonicalize(p).map_err(|e| format!("--sdk-path {p}: {e}"))?;
            format!("{{ path = {:?} }}", p.display().to_string())
        }
        None => SDK_GIT.to_string(),
    };
    let fill = |t: &str| {
        t.replace("{crate}", &id.replace('_', "-"))
            .replace("{id}", &id)
            .replace("{name}", &name)
            .replace("{sdk}", &sdk)
    };
    let files = [
        ("Cargo.toml", include_str!("../template/Cargo.toml.in")),
        (MANIFEST, include_str!("../template/horizon-mod.toml")),
        ("src/lib.rs", include_str!("../template/src/lib.rs")),
        (".gitignore", include_str!("../template/gitignore")),
    ];
    for (file, text) in files {
        let to = dir.join(file);
        std::fs::create_dir_all(to.parent().expect("a file in the mod"))
            .map_err(|e| e.to_string())?;
        std::fs::write(&to, fill(text)).map_err(|e| format!("{}: {e}", to.display()))?;
    }
    Manifest::parse(&std::fs::read_to_string(dir.join(MANIFEST)).map_err(|e| e.to_string())?)?;
    println!(
        "Created {} (mod id {id}). Next: cd {path} && cargo horizon-mod run",
        dir.display()
    );
    Ok(())
}

/// Builds the crate in the current folder and packs `target/horizon-mod/<id>.hmod`.
fn build(release: bool) -> Result<PathBuf, String> {
    let manifest_text = std::fs::read_to_string(MANIFEST)
        .map_err(|e| format!("{MANIFEST}: {e} (run this in a mod's folder)"))?;
    let manifest = Manifest::parse(&manifest_text)?;
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let mut cmd = Command::new(cargo);
    cmd.args([
        "build",
        "--lib",
        "--target",
        TARGET,
        "--message-format=json-render-diagnostics",
    ]);
    if release {
        cmd.arg("--release");
    }
    // Panic messages keep the source paths of every crate built: the home folder (and cargo's
    // registry in it) is renamed in them.
    cmd.env("CARGO_ENCODED_RUSTFLAGS", rustflags_without_home());
    let out = cmd
        .stderr(Stdio::inherit())
        .output()
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(format!(
            "cargo build failed (is the target installed? rustup target add {TARGET})"
        ));
    }
    let core = String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
        .filter(|m| m["reason"] == "compiler-artifact")
        .flat_map(|m| m["filenames"].as_array().cloned().unwrap_or_default())
        .filter_map(|f| f.as_str().map(PathBuf::from))
        .rfind(|f| f.extension().is_some_and(|e| e == "wasm"))
        .ok_or("the build made no .wasm: is the crate a cdylib?")?;
    let wasm = componentize(&std::fs::read(&core).map_err(|e| e.to_string())?)?;
    if let Some(found) = personal(&wasm) {
        return Err(format!(
            "the mod would carry {found} (a path of this computer): build it outside your home folder"
        ));
    }
    // The game refuses a component importing capabilities the manifest lacks.
    manifest.check_imports(component_imports(&wasm)?.iter().map(String::as_str))?;
    let target_dir = core.ancestors().nth(3).ok_or("unexpected target folder")?;
    let dir = target_dir.join("horizon-mod");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(format!("{}.hmod", manifest.id));
    // Write then rename, so a running game never reads half a file.
    let partial = path.with_extension("hmod.partial");
    let files = content_files()?;
    let files: Vec<(&str, &[u8])> = files
        .iter()
        .map(|(name, bytes)| (name.as_str(), bytes.as_slice()))
        .collect();
    let hmod = Package::write_files(&manifest_text, Some(&wasm), &files)?;
    std::fs::write(&partial, hmod).map_err(|e| e.to_string())?;
    std::fs::rename(&partial, &path).map_err(|e| e.to_string())?;
    Ok(path)
}

/// The files under the crate's `assets/` and `data/`, by their path in the `.hmod`.
fn content_files() -> Result<Vec<(String, Vec<u8>)>, String> {
    fn walk(dir: &Path, name: &str, out: &mut Vec<(String, Vec<u8>)>) -> Result<(), String> {
        let mut entries: Vec<_> = std::fs::read_dir(dir)
            .map_err(|e| format!("{}: {e}", dir.display()))?
            .filter_map(Result::ok)
            .collect();
        entries.sort_by_key(|e| e.file_name());
        for e in entries {
            let path = e.path();
            let name = format!("{name}/{}", e.file_name().to_string_lossy());
            if path.is_dir() {
                walk(&path, &name, out)?;
            } else {
                out.push((name, std::fs::read(&path).map_err(|e| e.to_string())?));
            }
        }
        Ok(())
    }
    let mut out = Vec::new();
    for folder in ["assets", "data"] {
        if Path::new(folder).is_dir() {
            walk(Path::new(folder), folder, &mut out)?;
        }
    }
    Ok(out)
}

fn run(args: &[String]) -> Result<(), String> {
    let path = build(true)?;
    let game = flag(args, "--game")
        .map(str::to_string)
        .or_else(|| std::env::var("HORIZON_CLIENT").ok())
        .unwrap_or_else(|| "horizon-client".into());
    let extra = args.iter().skip_while(|a| *a != "--").skip(1);
    println!(
        "Starting {game} with {} (rebuild with `cargo horizon-mod build` to reload it)",
        path.display()
    );
    let status = Command::new(&game)
        .arg("--dev-mod")
        .arg(&path)
        .args(extra)
        .status()
        .map_err(|e| {
            format!("{game}: {e} (pass --game <path to horizon-client> or set HORIZON_CLIENT)")
        })?;
    status
        .success()
        .then_some(())
        .ok_or_else(|| format!("the game exited with {status}"))
}

/// The user's home folder and the cargo home (in it, usually).
fn home_paths() -> Vec<String> {
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_default();
    let cargo = std::env::var("CARGO_HOME")
        .unwrap_or_else(|_| Path::new(&home).join(".cargo").display().to_string());
    [home, cargo].into_iter().filter(|s| s.len() >= 3).collect()
}

/// The rustflags cargo would use, plus `--remap-path-prefix` for the home folder and the cargo
/// home (the later, more precise one wins), in `CARGO_ENCODED_RUSTFLAGS`' form.
fn rustflags_without_home() -> String {
    let mut flags: Vec<String> = match std::env::var("CARGO_ENCODED_RUSTFLAGS") {
        Ok(encoded) if !encoded.is_empty() => encoded.split('\x1f').map(str::to_owned).collect(),
        _ => std::env::var("RUSTFLAGS")
            .unwrap_or_default()
            .split_whitespace()
            .map(str::to_owned)
            .collect(),
    };
    let paths = home_paths();
    for (path, name) in paths.iter().zip(["home", "cargo-home"]) {
        flags.push(format!("--remap-path-prefix={path}={name}"));
    }
    flags.join("\x1f")
}

/// A path of this computer found in a built mod: the home or cargo home folder, either slash.
fn personal(wasm: &[u8]) -> Option<String> {
    let mut found = home_paths();
    found.extend(found.clone().iter().map(|p| p.replace('\\', "/")));
    found.into_iter().find(|s| {
        wasm.windows(s.len())
            .any(|w| w.eq_ignore_ascii_case(s.as_bytes()))
    })
}

/// Starling's mods folder on a phone (its game folder in shared storage).
const PHONE_MODS: &str = "/storage/emulated/0/Starling/horizon-mods";

/// adb: `ADB`, the Android SDK's (`ANDROID_HOME`, or the default install), else the PATH's.
fn adb() -> PathBuf {
    if let Some(adb) = std::env::var_os("ADB") {
        return adb.into();
    }
    let exe = if cfg!(windows) { "adb.exe" } else { "adb" };
    let sdk = std::env::var_os("ANDROID_HOME")
        .or_else(|| std::env::var_os("ANDROID_SDK_ROOT"))
        .map(PathBuf::from)
        .or_else(|| {
            if cfg!(windows) {
                std::env::var_os("LOCALAPPDATA").map(|d| PathBuf::from(d).join("Android/Sdk"))
            } else if cfg!(target_os = "macos") {
                std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Android/sdk"))
            } else {
                std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Android/Sdk"))
            }
        });
    sdk.map(|s| s.join("platform-tools").join(exe))
        .filter(|p| p.is_file())
        .unwrap_or_else(|| exe.into())
}

fn push(args: &[String]) -> Result<(), String> {
    let path = build(!args.iter().any(|a| a == "--debug"))?;
    let folder = flag(args, "--to")
        .unwrap_or(PHONE_MODS)
        .trim_end_matches('/');
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or("unexpected mod file name")?;
    let (partial, target) = (
        format!("{folder}/{name}.partial"),
        format!("{folder}/{name}"),
    );
    let adb = adb();
    let run = |args: &[&str]| -> Result<(), String> {
        let status = Command::new(&adb)
            .args(args)
            .stdout(Stdio::null())
            .status()
            .map_err(|e| format!("{}: {e} (set ADB or ANDROID_HOME)", adb.display()))?;
        status
            .success()
            .then_some(())
            .ok_or_else(|| format!("adb {} failed ({status})", args.join(" ")))
    };
    // Copied under another name, then renamed: the game never reads half a mod.
    run(&["push", &path.to_string_lossy(), &partial])?;
    run(&["shell", "mv", "-f", &partial, &target])?;
    println!(
        "Pushed {name} to {folder}: the game reloads it (turn it on in Options > Mods the first time)"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn home_paths_are_found_in_a_mod() {
        let Some(home) = home_paths().into_iter().next() else {
            return;
        };
        let mut wasm = b"\0asm panicked at ".to_vec();
        wasm.extend(home.replace('\\', "/").as_bytes());
        wasm.extend(b"/src/lib.rs");
        assert!(personal(&wasm).is_some());
        assert_eq!(personal(b"\0asm crates/my-mod/src/lib.rs"), None);
        assert!(rustflags_without_home().contains(&format!("--remap-path-prefix={home}=home")));
    }

    #[test]
    fn the_template_is_a_valid_mod() {
        let dir = std::env::temp_dir().join(format!("horizon-mod-new-{}", std::process::id()));
        let path = dir.join("My-Mod");
        new(&[path.display().to_string(), "--name".into(), "My Mod".into()]).unwrap();
        let manifest =
            Manifest::parse(&std::fs::read_to_string(path.join(MANIFEST)).unwrap()).unwrap();
        assert_eq!(
            (manifest.id.as_str(), manifest.name.as_str()),
            ("my_mod", "My Mod")
        );
        let cargo = std::fs::read_to_string(path.join("Cargo.toml")).unwrap();
        assert!(cargo.contains("name = \"my-mod\"") && cargo.contains(SDK_GIT));
        assert!(
            new(&[path.display().to_string()]).is_err(),
            "never overwrites"
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
