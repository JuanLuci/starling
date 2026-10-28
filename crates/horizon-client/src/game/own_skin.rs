//! The player's own name and skin from Options > Skin Customization (horizon_client::own_skin):
//! the name kept for the next world or server, and the chosen PNG of `skins/` put in one of the
//! atlas's skin slots, where the local player's model (first and third person, the inventory's)
//! and the player list's head take it in place of the profile's skin (`update_remote_skins`).
use super::Game;
use horizon_client::{
    hud::{HudBatch, rgba},
    mobs::Mob,
    offline_name,
    own_skin::{self, SkinChoice},
    title::Mode,
};
use horizon_core::geometry::Vec3;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// The own skin's name in the atlas: one skin slot, written again when the skin changes.
const LOCATION: &str = "horizon/own_skin";

/// GUI pixels per block of the title screen's player (the inventory's is 30 too).
const TITLE_SIZE: f32 = 30.0;

/// The chosen skin and its image, read on first use.
#[derive(Default)]
pub(super) struct OwnSkin {
    loaded: bool,
    choice: SkinChoice,
    /// The chosen skin's 64x64 RGBA; None for the default one.
    image: Option<Vec<u8>>,
    /// Whether the slot holds `image` as it is now.
    uploaded: bool,
    /// The title screen's player, read again every second.
    title: Option<TitlePlayer>,
    /// Ely.by's skins by name, and whether Skin Customization has them on (read on first use).
    elyby: horizon_client::elyby::Lookup,
    elyby_on: Option<bool>,
    /// The name worlds open with, read again every few seconds.
    worlds_name: Option<(String, Instant)>,
}

/// The title screen's player: the model wearing the own skin, and the name worlds open with.
pub(super) struct TitlePlayer {
    mob: Mob,
    name: String,
    read: Instant,
}

/// The title screen's player box (GUI pixels), right of the menu's buttons and level with them
/// (title.rs, Mode::Main: 200 wide in the middle, from a quarter of the height plus 48); None
/// where the screen leaves it too little room.
fn title_player_box(w: f32, h: f32) -> Option<[f32; 4]> {
    let (w, h) = (w.ceil(), h.ceil());
    let left = (w / 2.0).floor() + 100.0;
    if w - left < 80.0 {
        return None;
    }
    let mid = ((left + w) / 2.0).floor();
    let top = (h / 4.0).floor() + 48.0 - 14.0;
    Some([mid - 30.0, top, mid + 30.0, top + 80.0])
}

impl Game<'_> {
    /// Where horizon-player.txt is: beside options.txt.
    fn player_dir() -> PathBuf {
        Self::options_path()
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_default()
    }

    /// `skins/` beside options.txt.
    pub(super) fn skins_dir() -> PathBuf {
        Self::options_path().with_file_name(own_skin::DIR)
    }

    /// Wear `choice`: its image read now; why not, for a file that is no skin (the default skin
    /// shows then).
    fn put_on_skin(&mut self, choice: SkinChoice) -> Result<(), String> {
        let skin = &mut self.own_skin;
        skin.loaded = true;
        skin.image = None;
        skin.uploaded = false;
        skin.choice = choice;
        let Some(file) = skin.choice.file.clone() else {
            return Ok(());
        };
        skin.image = Some(own_skin::load(&Self::skins_dir(), &file)?);
        Ok(())
    }

    /// The local player's own skin in the atlas, and whether its model is slim; None for the
    /// profile's skin (the account's, else the default one).
    pub(super) fn own_skin_sprite(&mut self) -> Option<(String, bool)> {
        if !self.own_skin.loaded
            && let Err(e) = self.put_on_skin(SkinChoice::load(&Self::player_dir()))
        {
            log::warn!("Horizon: own skin: {e}");
        }
        // None for the default skin.
        self.own_skin.image.as_ref()?;
        // In its slot, marked shown; else (changed, evicted, or the atlas made again with the
        // packs) written there again.
        if !self.own_skin.uploaded
            || !self.gui.atlas.sprites.contains_key(LOCATION)
            || !self.show_skin(LOCATION)
        {
            let rgba = self.own_skin.image.clone()?;
            self.store_skin(LOCATION.to_owned(), rgba);
            self.own_skin.uploaded = true;
        }
        Some((LOCATION.to_owned(), self.own_skin.choice.slim))
    }

    /// Skin Customization's name: kept for the next world or server.
    pub(super) fn keep_player_name(&mut self, name: &str) {
        if let Err(e) = offline_name::choose(&Self::player_dir(), name) {
            log::warn!("Horizon: {e}");
            self.ui.options.skin_warning(e);
        }
        self.own_skin.worlds_name = None;
        self.own_skin.title = None;
    }

    /// Skin Customization's Ely.by Skins: kept, and the skins shown again.
    pub(super) fn keep_elyby(&mut self, on: bool) {
        if let Err(e) = horizon_client::elyby::set_enabled(&Self::player_dir(), on) {
            log::warn!("Horizon: could not keep Ely.by Skins: {e}");
        }
        self.own_skin.elyby_on = Some(on);
        self.own_skin.title = None;
    }

    fn elyby_enabled(&mut self) -> bool {
        *self
            .own_skin
            .elyby_on
            .get_or_insert_with(|| horizon_client::elyby::enabled(&Self::player_dir()))
    }

    /// `name`'s skin on Ely.by, where Skin Customization has Ely.by Skins on and it answered.
    pub(super) fn elyby_textures(&mut self, name: &str) -> Option<horizon_client::skins::Textures> {
        if !self.elyby_enabled() {
            return None;
        }
        self.own_skin.elyby.textures(name)
    }

    /// The name worlds open with (`offline_name::for_worlds`), read at most every few seconds.
    fn worlds_name(&mut self) -> String {
        if let Some((name, read)) = &self.own_skin.worlds_name
            && read.elapsed() < Duration::from_secs(5)
        {
            return name.clone();
        }
        let name = offline_name::for_worlds(&Self::player_dir());
        self.own_skin.worlds_name = Some((name.clone(), Instant::now()));
        name
    }

    /// The skin the player wears where no server gives one (single player, the title screen):
    /// the file Skin Customization chose, else, playing offline, the one the name worlds open
    /// with has on Ely.by. None for the default skin.
    pub(super) fn worn_skin(&mut self) -> Option<(String, bool)> {
        if let Some(own) = self.own_skin_sprite() {
            return Some(own);
        }
        if horizon_client::auth::current().is_some() {
            return None;
        }
        let name = self.worlds_name();
        let textures = self.elyby_textures(&name)?;
        let location = {
            let mut downloads = self.skin_downloads.borrow_mut();
            horizon_client::skins::lookup(&textures, false, |kind, url| downloads.state(kind, url))
        }?;
        // In its atlas slot (again, if the slot went to another skin).
        if !self.again_skin(&location) {
            return None;
        }
        self.store_ready_skins();
        self.gui
            .atlas
            .sprites
            .contains_key(&location)
            .then_some((location, textures.slim))
    }

    /// The skins downloaded since the last call, put in the atlas's skin slots.
    pub(super) fn store_ready_skins(&mut self) {
        let ready: Vec<_> = self.skin_downloads.borrow_mut().take_ready();
        for (location, rgba) in ready {
            self.store_skin(location, rgba);
        }
    }

    /// Skin Customization's skin or model: kept and put on.
    pub(super) fn choose_own_skin(&mut self, choice: SkinChoice) {
        if let Err(e) = choice.save(&Self::player_dir()) {
            log::warn!("Horizon: could not keep the skin: {e}");
            self.ui
                .options
                .skin_warning(format!("Could not keep the skin: {e}"));
        }
        if let Err(e) = self.put_on_skin(choice) {
            log::warn!("Horizon: own skin: {e}");
            self.ui.options.skin_warning(e);
        }
    }

    /// Skin Customization's Choose a File...: the system's picker, whose file comes through the
    /// import inbox (`own_skin_imported`).
    pub(super) fn pick_skin_file(&mut self) {
        if !horizon_client::platform::pick_file() {
            self.ui
                .options
                .skin_warning("No file picker opened: put the skin in the skins folder".into());
        }
    }

    /// A skin the import inbox put in `skins/`: worn at once, the page showing it.
    pub(super) fn own_skin_imported(&mut self, path: &Path) {
        let Some(file) = path.file_name().and_then(|f| f.to_str()) else {
            return;
        };
        if !self.own_skin.loaded {
            let _ = self.put_on_skin(SkinChoice::load(&Self::player_dir()));
        }
        let choice = SkinChoice {
            file: Some(file.to_owned()),
            slim: self.own_skin.choice.slim,
        };
        if let Err(e) = choice.save(&Self::player_dir()) {
            log::warn!("Horizon: could not keep the skin: {e}");
        }
        let worn = self.put_on_skin(choice);
        self.ui.options.refresh_skin_page();
        if let Err(e) = worn {
            log::warn!("Horizon: own skin: {e}");
            self.ui.options.skin_warning(e);
        }
    }

    /// Where the title screen shows its player in a GUI `w` by `h`; None on its other pages.
    fn title_player_rect(&self, w: f32, h: f32) -> Option<[f32; 4]> {
        let title = self.title.as_ref()?;
        matches!(title.mode, Mode::Main)
            .then(|| title_player_box(w, h))
            .flatten()
    }

    /// A press on the title screen's player, its name or its hint: Skin Customization opens.
    pub(super) fn title_player_at(&self, (x, y): (f32, f32)) -> bool {
        let Some(r) = &self.renderer else {
            return false;
        };
        let b = HudBatch::new([r.config.width, r.config.height]);
        self.title_player_rect(b.width, b.height)
            .is_some_and(|[x0, y0, x1, y1]| {
                x >= x0 - 10.0 && x < x1 + 10.0 && y >= y0 && y < y1 + 22.0
            })
    }

    /// The title screen's player beside the menu, as Bedrock's: the own skin, the name worlds
    /// open with, and the hint that a press changes them.
    pub(super) fn draw_title_player(&mut self, batch: &mut HudBatch) {
        let Some(rect) = self.title_player_rect(batch.width, batch.height) else {
            return;
        };
        if self
            .own_skin
            .title
            .as_ref()
            .is_none_or(|t| t.read.elapsed() > Duration::from_secs(1))
        {
            self.read_title_player();
        }
        let Some(player) = &self.own_skin.title else {
            return;
        };
        let [x0, y0, x1, y1] = rect;
        let centre = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
        // Turned toward the mouse; ahead on a touch screen.
        let cursor = if self.has_touch() {
            centre
        } else {
            self.cursor
        };
        self.mob_preview(&player.mob, batch, rect, TITLE_SIZE, 0.0625, cursor, 0.0);
        let atlas = &self.gui.atlas;
        let mut line = |text: &str, y: f32, color: u32| {
            let x = (centre.0 - HudBatch::text_width(atlas, text) / 2.0).floor();
            batch.text(atlas, text, x, y, color, true);
        };
        line(&player.name, y1 + 2.0, u32::MAX);
        let hint = if self.has_touch() {
            "Tap to customize"
        } else {
            "Click to customize"
        };
        line(hint, y1 + 12.0, rgba(160, 160, 160, 255));
    }

    /// The title screen's player as Skin Customization and the sign-in leave it now.
    fn read_title_player(&mut self) {
        let Some(kind) = self.res.player_type().clone() else {
            return;
        };
        let worn = self.worn_skin();
        let mut mob = Mob::new(0, kind, Vec3::ZERO, 0.0);
        mob.skin = worn.as_ref().map(|(skin, _)| skin.clone());
        mob.slim = worn.is_some_and(|(_, slim)| slim);
        self.own_skin.title = Some(TitlePlayer {
            mob,
            name: self.worlds_name(),
            read: Instant::now(),
        });
    }
}
