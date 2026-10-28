//! The player's own skin (Options > Skin Customization): the PNGs of `skins/` beside options.txt,
//! the one chosen and its model, kept in horizon-player.txt (`offline_name`), and the image read
//! as vanilla reads a downloaded skin (`SkinTextureDownloader.processLegacySkin`: 64x64, or a
//! legacy 64x32 laid out anew). Only this game draws it: other players see the skin the server
//! sends for the profile, as vanilla's protocol has it.
use crate::offline_name;
use std::io::Read;
use std::path::Path;

/// The folder beside options.txt holding the skins to choose from.
pub const DIR: &str = "skins";
const SKIN: &str = "skin";
const MODEL: &str = "model";

/// The chosen skin: a file of `skins/` (None: the default skin) and its model.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SkinChoice {
    pub file: Option<String>,
    /// The slim arms (Alex's) rather than the classic ones (Steve's).
    pub slim: bool,
}

impl SkinChoice {
    /// The choice kept in `dir`'s horizon-player.txt.
    pub fn load(dir: &Path) -> Self {
        Self {
            file: offline_name::value(dir, SKIN).filter(|f| is_file_name(f)),
            slim: offline_name::value(dir, MODEL).as_deref() == Some("slim"),
        }
    }

    /// Keep the choice in `dir`'s horizon-player.txt, beside the name.
    pub fn save(&self, dir: &Path) -> std::io::Result<()> {
        let model = if self.slim { "slim" } else { "classic" };
        offline_name::set_values(dir, &[(SKIN, self.file.as_deref()), (MODEL, Some(model))])
    }
}

/// What the page calls a skin file: its name without `.png`.
pub fn shown_name(file: &str) -> &str {
    match file.len().checked_sub(4) {
        Some(at) if file.is_char_boundary(at) && file[at..].eq_ignore_ascii_case(".png") => {
            &file[..at]
        }
        _ => file,
    }
}

/// A plain file name of `skins/`, never a path out of it.
fn is_file_name(name: &str) -> bool {
    !name.is_empty() && !name.contains(['/', '\\']) && name != "." && name != ".."
}

/// Whether `bytes` start as a PNG file does.
pub fn is_png(bytes: &[u8]) -> bool {
    bytes.starts_with(b"\x89PNG\r\n\x1a\n")
}

/// A PNG's width and height, from its header (IHDR) alone.
fn png_size(bytes: &[u8]) -> Option<(u32, u32)> {
    if !is_png(bytes) || bytes.get(12..16)? != b"IHDR" {
        return None;
    }
    let int = |at: usize| Some(u32::from_be_bytes(bytes.get(at..at + 4)?.try_into().ok()?));
    Some((int(16)?, int(20)?))
}

/// Why an image of this size is no skin; None for 64x64 and the legacy 64x32.
fn size_error(width: u32, height: u32) -> Option<String> {
    (width != 64 || height != 64 && height != 32)
        .then(|| format!("it is {width}x{height}, and a skin is 64x64 (or 64x32)"))
}

/// The skins to choose from: the PNG files of `skins` of a skin's size, by name.
pub fn list(skins: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(skins) else {
        return Vec::new();
    };
    let mut files: Vec<String> = entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            if !name.to_ascii_lowercase().ends_with(".png") || !entry.file_type().ok()?.is_file() {
                return None;
            }
            let mut header = [0u8; 24];
            std::fs::File::open(entry.path())
                .ok()?
                .read_exact(&mut header)
                .ok()?;
            let (width, height) = png_size(&header)?;
            size_error(width, height).is_none().then_some(name)
        })
        .collect();
    files.sort_by_key(|f| f.to_lowercase());
    files
}

/// The skin after `current` in the page's cycle: the default (None), then each of `files`.
pub fn next(files: &[String], current: Option<&str>) -> Option<String> {
    let after = current
        .and_then(|c| files.iter().position(|f| f == c))
        .map_or(0, |i| i + 1);
    files.get(after).cloned()
}

/// A skin PNG as the game draws it, 64x64 RGBA; why not, for a file that is no skin.
pub fn decode(bytes: &[u8]) -> Result<Vec<u8>, String> {
    if !is_png(bytes) {
        return Err("it is not a PNG image".into());
    }
    // The header first: a photo picked by mistake is not decoded whole.
    if let Some(error) = png_size(bytes).and_then(|(w, h)| size_error(w, h)) {
        return Err(error);
    }
    let (width, height, rgba) =
        crate::hud::decode_bytes(bytes).map_err(|e| format!("it could not be read ({e})"))?;
    if let Some(error) = size_error(width, height) {
        return Err(error);
    }
    crate::skins::process_legacy_skin(width, height, &rgba)
        .ok_or_else(|| format!("it is {width}x{height}, and a skin is 64x64 (or 64x32)"))
}

/// The file `file` of `skins`, read for drawing.
pub fn load(skins: &Path, file: &str) -> Result<Vec<u8>, String> {
    if !is_file_name(file) {
        return Err(format!("{file} is not a file of the skins folder"));
    }
    let bytes =
        std::fs::read(skins.join(file)).map_err(|e| format!("could not read {file}: {e}"))?;
    decode(&bytes).map_err(|e| format!("{file} is not a skin: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn scratch(name: &str) -> std::path::PathBuf {
        let d =
            std::env::temp_dir().join(format!("horizon-own-skin-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn png_of(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut encoder = png::Encoder::new(&mut out, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(rgba).unwrap();
        writer.finish().unwrap();
        out
    }

    fn pixel(rgba: &[u8], x: usize, y: usize) -> [u8; 4] {
        let at = (y * 64 + x) * 4;
        rgba[at..at + 4].try_into().unwrap()
    }

    #[test]
    fn a_legacy_skin_gets_the_64x64_layout() {
        // A 64x32 skin: the right leg's front (4..8, 20..32) and arm's front (44..48, 20..32),
        // a see-through head and a fully opaque hat layer.
        let mut rgba = vec![0u8; 64 * 32 * 4];
        let mut set = |x: usize, y: usize, p: [u8; 4]| {
            let at = (y * 64 + x) * 4;
            rgba[at..at + 4].copy_from_slice(&p);
        };
        for y in 0..32 {
            for x in 32..64 {
                set(x, y, [9, 9, 9, 255]);
            }
        }
        set(4, 20, [255, 0, 0, 255]);
        set(7, 20, [0, 0, 255, 255]);
        set(44, 20, [0, 255, 0, 255]);
        set(8, 8, [1, 2, 3, 0]);
        let skin = decode(&png_of(64, 32, &rgba)).unwrap();
        assert_eq!(skin.len(), 64 * 64 * 4);
        // The left leg (20..24, 52..64) and left arm (36..40, 52..64) mirror the right ones.
        assert_eq!(pixel(&skin, 23, 52), [255, 0, 0, 255]);
        assert_eq!(pixel(&skin, 20, 52), [0, 0, 255, 255]);
        assert_eq!(pixel(&skin, 39, 52), [0, 255, 0, 255]);
        // The head is opaque; an opaque hat layer is cleared (doNotchTransparencyHack).
        assert_eq!(pixel(&skin, 8, 8), [1, 2, 3, 255]);
        assert_eq!(pixel(&skin, 40, 8)[3], 0);
        // The legacy image's own pixels stay where they were.
        assert_eq!(pixel(&skin, 4, 20), [255, 0, 0, 255]);
    }

    #[test]
    fn only_skin_sized_pngs_are_skins() {
        let square = vec![200u8; 64 * 64 * 4];
        assert_eq!(decode(&png_of(64, 64, &square)).unwrap().len(), 64 * 64 * 4);
        let error = decode(&png_of(32, 32, &[0; 32 * 32 * 4])).unwrap_err();
        assert!(error.contains("32x32"), "{error}");
        assert!(decode(&png_of(128, 128, &[0; 128 * 128 * 4])).is_err());
        assert!(decode(b"PK\x03\x04 a zip").is_err());

        let d = scratch("list");
        fs::write(d.join("steve.png"), png_of(64, 64, &square)).unwrap();
        fs::write(d.join("Old.PNG"), png_of(64, 32, &[0; 64 * 32 * 4])).unwrap();
        fs::write(d.join("big.png"), png_of(128, 128, &[0; 128 * 128 * 4])).unwrap();
        fs::write(d.join("notes.txt"), b"hello").unwrap();
        fs::write(d.join("fake.png"), b"not an image").unwrap();
        let files = list(&d);
        assert_eq!(files, ["Old.PNG", "steve.png"]);
        assert_eq!(load(&d, "steve.png").unwrap().len(), 64 * 64 * 4);
        assert!(load(&d, "../steve.png").is_err());
        assert!(load(&d, "big.png").is_err());
        assert_eq!(shown_name("Old.PNG"), "Old");
        assert_eq!(shown_name("é"), "é");
        // The cycle: the default, each skin, the default again.
        assert_eq!(next(&files, None).as_deref(), Some("Old.PNG"));
        assert_eq!(next(&files, Some("Old.PNG")).as_deref(), Some("steve.png"));
        assert_eq!(next(&files, Some("steve.png")), None);
        assert_eq!(next(&files, Some("gone.png")).as_deref(), Some("Old.PNG"));
        assert_eq!(next(&[], None), None);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn the_choice_is_kept_beside_the_name() {
        let d = scratch("choice");
        assert_eq!(SkinChoice::load(&d), SkinChoice::default());
        offline_name::choose(&d, "Alex_9").unwrap();
        let choice = SkinChoice {
            file: Some("alex.png".into()),
            slim: true,
        };
        choice.save(&d).unwrap();
        assert_eq!(SkinChoice::load(&d), choice);
        assert_eq!(offline_name::chosen(&d).as_deref(), Some("Alex_9"));
        let default = SkinChoice::default();
        default.save(&d).unwrap();
        assert_eq!(SkinChoice::load(&d), default);
        assert_eq!(
            fs::read_to_string(d.join(offline_name::FILE)).unwrap(),
            "name:Alex_9\nchosen:true\nmodel:classic\n"
        );
        // A path in the file is not a skin of the folder.
        fs::write(d.join(offline_name::FILE), "skin:../x.png\nmodel:slim\n").unwrap();
        assert_eq!(
            SkinChoice::load(&d),
            SkinChoice {
                file: None,
                slim: true
            }
        );
        let _ = fs::remove_dir_all(&d);
    }
}
