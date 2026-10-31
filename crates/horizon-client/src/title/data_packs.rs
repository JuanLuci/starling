//! Starling: Create New World's Data Packs screen (`CreateWorldScreen.openDataPackSelectionScreen`):
//! the pack screen over the game's `datapacks` folder, where a player puts packs (a phone's file
//! manager reaches it in the game's shared folder). The chosen ones are copied into the new
//! world's `datapacks`, where the world finds and enables them.
use crate::hud::{HudAtlas, HudBatch};
use crate::pack_screen::{self, Action, Compat, PackInfo, PackScreen, Source};
use crate::packs::Position;
use crate::screen_draw::Measure;
use crate::text::{Style, Text};
use std::path::{Path, PathBuf};
use std::time::Instant;

/// The game's own folder of data packs to choose from, beside `resourcepacks`.
pub fn folder() -> PathBuf {
    std::env::current_dir()
        .unwrap_or_default()
        .join("datapacks")
}

/// The packs the screen offers: the folder's, then vanilla's own (required, at the bottom).
fn infos(dir: &Path) -> Vec<PackInfo> {
    // A folder of its own as `resources`: no built-in resource packs come along.
    let mut infos: Vec<PackInfo> = crate::packs::discover(dir, dir)
        .iter()
        .map(|p| PackInfo {
            id: p.id.clone(),
            title: p.title.clone(),
            description: p.raw_description.clone(),
            description_style: p.description_style.clone(),
            source: Source::Default,
            // The resource pack format it is judged by is not a data pack's.
            compat: Compat::Compatible,
            icon: p.icon_png().as_deref().and_then(pack_screen::load_icon),
            required: false,
            position: Position::Top,
            fixed: false,
        })
        .collect();
    infos.push(PackInfo {
        id: "vanilla".into(),
        title: Text::literal("vanilla"),
        description: Text::translate("dataPack.vanilla.description", &[]),
        description_style: Style::default(),
        source: Source::BuiltIn,
        compat: Compat::Compatible,
        icon: None,
        required: true,
        position: Position::Bottom,
        fixed: false,
    });
    infos
}

/// The open screen, made at its first frame (it measures text), and what was typed into its
/// search box since it last drew (the title's typing has no font).
pub(super) struct DataPacks {
    opening: Option<(Vec<PackInfo>, Vec<String>)>,
    screen: Option<PackScreen>,
    typed: String,
    erased: isize,
    focused: Instant,
}

impl DataPacks {
    /// Opened over Create New World with its current choice.
    pub(super) fn open(chosen: &[String]) -> Self {
        let dir = folder();
        let selected = std::iter::once("vanilla".to_owned())
            .chain(chosen.iter().cloned())
            .collect();
        DataPacks {
            opening: Some((infos(&dir), selected)),
            screen: None,
            typed: String::new(),
            erased: 0,
            focused: Instant::now(),
        }
    }
    fn screen(&mut self, m: &Measure, (w, h): (i32, i32)) -> &mut PackScreen {
        if let Some((infos, selected)) = self.opening.take() {
            self.screen = Some(PackScreen::new(m, w, h, false, infos, &selected).for_data(m));
        }
        let screen = self.screen.get_or_insert_with(|| {
            PackScreen::new(m, w, h, false, infos(&folder()), &["vanilla".to_owned()]).for_data(m)
        });
        if screen.width != w || screen.height != h {
            screen.resize(m, w, h);
        }
        if !self.typed.is_empty() {
            screen.type_text(m, &std::mem::take(&mut self.typed));
        }
        if self.erased > 0 {
            screen.erase(m, std::mem::take(&mut self.erased));
        }
        screen
    }
    pub(super) fn draw(&mut self, atlas: &HudAtlas, batch: &mut HudBatch, cursor: (f32, f32)) {
        let (w, h) = (batch.width.ceil(), batch.height.ceil());
        crate::ui::menu_background(batch, atlas, 0.0, 0.0, w, h, false);
        let m = Measure(&atlas.font);
        let blink = self.focused.elapsed().as_millis() as u64;
        let screen = self.screen(&m, (w as i32, h as i32));
        let mouse = (cursor.0.floor() as i32, cursor.1.floor() as i32);
        let mut draws = screen.draw(&m, mouse, blink);
        screen.bind_icons(&mut draws, atlas, &mut batch.patches);
        crate::screen_draw::paint(batch, atlas, &draws);
    }
    /// A left press: whether it lands on a button (its sound), and the choice when Done
    /// closes the screen (`PackSelectionModel.commit`, vanilla's own left out).
    pub(super) fn press(
        &mut self,
        atlas: &HudAtlas,
        cursor: (f32, f32),
        size: (f32, f32),
    ) -> (bool, Option<Vec<String>>) {
        let m = Measure(&atlas.font);
        let screen = self.screen(&m, (size.0.ceil() as i32, size.1.ceil() as i32));
        let (x, y) = (f64::from(cursor.0), f64::from(cursor.1));
        let sound = screen.button_at(&m, x, y);
        let was_focused = screen.search_focused();
        let action = screen.click(&m, x, y);
        if screen.search_focused() && !was_focused {
            self.focused = Instant::now();
        }
        match action {
            Action::OpenFolder => {
                if let Err(e) = super::world_management::open_folder(&folder()) {
                    log::warn!("Horizon: could not open the data packs folder: {e}");
                }
                (sound, None)
            }
            Action::Done => (sound, Some(self.commit())),
            Action::None => (sound, None),
        }
    }
    /// The choice Escape closes the screen with (`onClose`: commit). Every pack is shown
    /// compatible, so no confirmation is ever open.
    pub(super) fn commit(&self) -> Vec<String> {
        match (&self.screen, &self.opening) {
            (Some(screen), _) => screen.commit(),
            (None, Some((_, selected))) => selected.clone(),
            (None, None) => Vec::new(),
        }
        .into_iter()
        .filter(|id| id != "vanilla")
        .collect()
    }
    pub(super) fn type_text(&mut self, text: &str) {
        self.typed.push_str(text);
    }
    pub(super) fn backspace(&mut self) {
        if self.typed.pop().is_none() {
            self.erased += 1;
        }
    }
    pub(super) fn drag(&mut self, y: f32) {
        if let Some(s) = &mut self.screen {
            s.drag(f64::from(y));
        }
    }
    pub(super) fn release(&mut self) {
        if let Some(s) = &mut self.screen {
            s.release();
        }
    }
    pub(super) fn wheel(&mut self, delta: f32, cursor: (f32, f32)) {
        if let Some(s) = &mut self.screen {
            s.scroll(f64::from(cursor.0), f64::from(cursor.1), f64::from(delta));
        }
    }
    /// Whether its search box has the focus.
    pub(super) fn typing(&self) -> bool {
        self.screen.as_ref().is_some_and(PackScreen::search_focused)
    }
    /// Whether `(x, y)` is on the focused search box, where a tap asks for a phone's keyboard.
    pub(super) fn search_at(&self, (x, y): (f32, f32)) -> bool {
        self.screen.as_ref().is_some_and(|s| s.search_at(x, y))
    }
}

/// The chosen packs (`file/<name>`) copied from `from` (the game's folder) into the world's
/// `datapacks`.
pub(super) fn copy_into(from: &Path, world: &Path, chosen: &[String]) {
    let to = world.join("datapacks");
    for name in chosen.iter().filter_map(|id| id.strip_prefix("file/")) {
        let source = from.join(name);
        if let Err(e) = std::fs::create_dir_all(&to).and_then(|()| copy(&source, &to.join(name))) {
            log::warn!("Horizon: could not copy data pack {name}: {e}");
        }
    }
}

fn copy(from: &Path, to: &Path) -> std::io::Result<()> {
    if from.is_dir() {
        std::fs::create_dir_all(to)?;
        for e in std::fs::read_dir(from)? {
            let e = e?;
            copy(&e.path(), &to.join(e.file_name()))?;
        }
        Ok(())
    } else {
        std::fs::copy(from, to).map(|_| ())
    }
}
