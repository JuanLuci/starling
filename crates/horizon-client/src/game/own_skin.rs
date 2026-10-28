//! The player's own name and skin from Options > Skin Customization (horizon_client::own_skin):
//! the name kept for the next world or server, and the chosen PNG of `skins/` put in one of the
//! atlas's skin slots, where the local player's model (first and third person, the inventory's)
//! and the player list's head take it in place of the profile's skin (`update_remote_skins`).
use super::Game;
use horizon_client::{
    offline_name,
    own_skin::{self, SkinChoice},
};
use std::path::{Path, PathBuf};

/// The own skin's name in the atlas: one skin slot, written again when the skin changes.
const LOCATION: &str = "horizon/own_skin";

/// The chosen skin and its image, read on first use.
#[derive(Default)]
pub(super) struct OwnSkin {
    loaded: bool,
    choice: SkinChoice,
    /// The chosen skin's 64x64 RGBA; None for the default one.
    image: Option<Vec<u8>>,
    /// Whether the slot holds `image` as it is now.
    uploaded: bool,
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
}
