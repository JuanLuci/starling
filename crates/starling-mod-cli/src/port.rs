//! `starling-mod pack-port <carried folder> --jar <jar> --modrinth <slug> --version <version>
//! --out <file.hmod> [--take assets/<namespace>/]...`: a ported Java mod's `.hmod` to share,
//! without the assets its authors keep. The folders taken (the mod's own `assets/<id>/` unless
//! named) stay out, and the manifest gets an `[assets.fetch]` naming the jar by its SHA-512, so
//! the game downloads it from Modrinth on the player's device and unpacks them there
//! (horizon_mods::fetch). The jar is unpacked here as the game will: a carried file the game
//! would not get back the same is an error, so the shared mod looks as the local one did.
use horizon_mods::Manifest;
use horizon_mods::manifest::MANIFEST;
use serde_json::json;
use sha2::{Digest, Sha512};
use std::io::Write;
use std::path::{Path, PathBuf};

pub(crate) fn pack_port(args: &[String]) -> Result<(), String> {
    let mut carry = None;
    let (mut jar, mut modrinth, mut version, mut out) = (None, None, None, None);
    let mut take = Vec::new();
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        let mut value = || it.next().cloned().ok_or(format!("{arg} needs a value"));
        match arg.as_str() {
            "--jar" => jar = Some(PathBuf::from(value()?)),
            "--modrinth" => modrinth = Some(value()?),
            "--version" => version = Some(value()?),
            "--out" => out = Some(PathBuf::from(value()?)),
            "--take" => take.push(value()?),
            _ if carry.is_none() && !arg.starts_with("--") => carry = Some(PathBuf::from(arg)),
            _ => return Err(format!("pack-port: unknown argument {arg}")),
        }
    }
    let (Some(carry), Some(jar), Some(modrinth), Some(version), Some(out)) =
        (carry, jar, modrinth, version, out)
    else {
        return Err(
            "usage: starling-mod pack-port <carried folder> --jar <jar> --modrinth \
                    <slug> --version <version> --out <file.hmod> [--take assets/<ns>/]..."
                .into(),
        );
    };
    let text = std::fs::read_to_string(carry.join(MANIFEST))
        .map_err(|e| format!("{}: {e}", carry.join(MANIFEST).display()))?;
    let manifest = Manifest::parse(&text)?;
    if manifest.assets.is_some() {
        return Err(format!("{MANIFEST} already fetches its assets"));
    }
    if take.is_empty() {
        take.push(format!("assets/{}/", manifest.id));
    }
    let bytes = std::fs::read(&jar).map_err(|e| format!("{}: {e}", jar.display()))?;
    let sha512: String = Sha512::digest(&bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    let file = jar
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or("the jar's name is not UTF-8")?;
    let quoted: Vec<String> = take.iter().map(|t| format!("{t:?}")).collect();
    let text = format!(
        "{}\n# The assets are the authors' (All Rights Reserved): the game fetches them from this\n\
         # very file on Modrinth, on the player's device.\n[assets.fetch]\nmodrinth = {modrinth:?}\n\
         version = {version:?}\nfile = {file:?}\nsize = {}\nsha512 = \"{sha512}\"\ntake = [{}]\n",
        text.trim_end(),
        bytes.len(),
        quoted.join(", ")
    );
    let manifest = Manifest::parse(&text)?;
    // The jar unpacked as the game will, to compare with what was carried.
    let unpacked = std::env::temp_dir().join(format!("starling-pack-port-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&unpacked);
    let count = horizon_mods::fetch::unpack(&jar, &manifest, &unpacked)?;
    let mut files = Vec::new();
    walk(&carry, &carry, &mut files)?;
    files.sort();
    let mut kept = Vec::new();
    let mut differ = Vec::new();
    let mut left_out = 0usize;
    for name in &files {
        if name == MANIFEST {
            continue;
        }
        if !take.iter().any(|t| name.starts_with(t.as_str())) {
            kept.push(name.clone());
            continue;
        }
        left_out += 1;
        let ours = std::fs::read(carry.join(name)).map_err(|e| format!("{name}: {e}"))?;
        if std::fs::read(unpacked.join(name)).ok().as_ref() != Some(&ours) {
            differ.push(name.clone());
        }
    }
    let _ = std::fs::remove_dir_all(&unpacked);
    if !differ.is_empty() {
        return Err(format!(
            "{} carried files under {} would not come back the same from the jar (changed by \
             hand, or from elsewhere): {}",
            differ.len(),
            take.join(", "),
            differ
                .iter()
                .take(10)
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    write_zip(&out, &text, &carry, &kept)?;
    let size = std::fs::metadata(&out).map(|m| m.len()).unwrap_or(0);
    crate::emit(json!({
        "event": "packed",
        "out": out.display().to_string(),
        "bytes": size,
        "files": kept.len() + 1,
        "left_out": left_out,
        "fetched": count,
        "jar": file,
        "sha512": sha512,
    }));
    Ok(())
}

/// The `.hmod`: the manifest, then the files in name order, with no times (the same folder makes
/// the same file).
fn write_zip(out: &Path, manifest: &str, root: &Path, files: &[String]) -> Result<(), String> {
    let partial = out.with_extension("partial");
    let file =
        std::fs::File::create(&partial).map_err(|e| format!("{}: {e}", partial.display()))?;
    let mut zip = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .last_modified_time(zip::DateTime::default());
    let mut put = |name: &str, bytes: &[u8]| -> Result<(), String> {
        zip.start_file(name, options).map_err(|e| e.to_string())?;
        zip.write_all(bytes).map_err(|e| e.to_string())
    };
    put(MANIFEST, manifest.as_bytes())?;
    for name in files {
        let bytes = std::fs::read(root.join(name)).map_err(|e| format!("{name}: {e}"))?;
        put(name, &bytes)?;
    }
    zip.finish().map_err(|e| e.to_string())?;
    std::fs::rename(&partial, out).map_err(|e| format!("{}: {e}", out.display()))
}

/// Files under `dir`, relative to `root` with `/` separators.
fn walk(root: &Path, dir: &Path, out: &mut Vec<String>) -> Result<(), String> {
    for entry in std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))? {
        let path = entry.map_err(|e| e.to_string())?.path();
        if path.is_dir() {
            walk(root, &path, out)?;
        } else if path.is_file() {
            let rel = path.strip_prefix(root).map_err(|e| e.to_string())?;
            let parts: Option<Vec<&str>> = rel.iter().map(|p| p.to_str()).collect();
            out.push(parts.ok_or("a file name is not UTF-8")?.join("/"));
        }
    }
    Ok(())
}
