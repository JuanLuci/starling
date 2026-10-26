//! Resource pack selection bound to the game: the pack screen lists, and a
//! full resource reload after a change, while playing.
use super::Game;
use horizon_client::{
    hud::HudAtlas,
    lang,
    pack_screen::{self, Compat, PackInfo, Source as PackSource},
    packs::{self, Pack, Position},
    text::{Style, Text},
};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Instant,
};

/// Where overlay roots are built (beside options.txt, or in the private folder: `logging::cache_dir`).
pub fn cache_dir() -> PathBuf {
    horizon_client::logging::cache_dir().join("resourcepacks")
}
pub fn packs_dir() -> PathBuf {
    std::env::current_dir()
        .unwrap_or_default()
        .join("resourcepacks")
}
/// `Util.getPlatform().openPath`: the system file manager on the folder.
pub fn open_folder(dir: &Path) {
    // Android's file manager, through the activity.
    if horizon_client::platform::open_folder(dir).is_some() {
        return;
    }
    let program = if cfg!(target_os = "windows") {
        "explorer"
    } else if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    if let Err(e) = std::process::Command::new(program).arg(dir).spawn() {
        log::warn!("Horizon: could not open {}: {e}", dir.display());
    }
}
/// The resource root to load from, given the vanilla root, the selection and
/// a server's packs, which go on top (`Pack.Position.TOP`, fixed).
pub fn startup_root(
    vanilla: &Path,
    selected: &[String],
    server: &[Pack],
) -> std::io::Result<PathBuf> {
    let all = packs::discover(vanilla, &packs_dir());
    let mut enabled = packs::enabled(&all, selected);
    enabled.extend_from_slice(server);
    packs::resource_root(vanilla, &enabled, &cache_dir())
}
fn info(p: &Pack) -> PackInfo {
    let built_in = !p.id.starts_with("file/");
    PackInfo {
        id: p.id.clone(),
        title: p.title.clone(),
        description: p.raw_description.clone(),
        description_style: p.description_style.clone(),
        source: if built_in {
            PackSource::BuiltIn
        } else {
            PackSource::Default
        },
        compat: p.compatibility,
        icon: p.icon_png().as_deref().and_then(pack_screen::load_icon),
        required: false,
        position: Position::Top,
        fixed: false,
    }
}
/// The vanilla pack: required, at the bottom (`BuiltInPackSource`), with the
/// client JAR's `pack.png` that the prepared resources hold.
fn vanilla_info(vanilla: &Path) -> PackInfo {
    PackInfo {
        id: "vanilla".into(),
        title: Text::translate("resourcePack.vanilla.name", &[]),
        description: Text::translate("resourcePack.vanilla.description", &[]),
        description_style: Style::default(),
        source: PackSource::BuiltIn,
        compat: Compat::Compatible,
        icon: std::fs::read(vanilla.join("pack.png"))
            .ok()
            .as_deref()
            .and_then(pack_screen::load_icon),
        required: true,
        position: Position::Bottom,
        fixed: false,
    }
}
/// Shared by the startup menu, which has no world resources yet.
pub(super) fn pack_infos(vanilla: &Path) -> Vec<PackInfo> {
    let mut infos: Vec<PackInfo> = packs::discover(vanilla, &packs_dir())
        .iter()
        .map(info)
        .collect();
    infos.push(vanilla_info(vanilla));
    infos
}
/// A resource reload preparing on a worker (`Game::reload_selected_packs`).
pub(super) struct PackReload {
    handle: std::thread::JoinHandle<Result<PreparedPacks, String>>,
    /// For the server's packs, which hear how it went.
    server: bool,
    started: Instant,
}
/// What a reload reads from the new resource root, off the main thread.
pub(super) struct PreparedPacks {
    root: PathBuf,
    gui: super::UiResources,
    pack: super::PackResources,
    textures: horizon_client::gfx::PreparedTextures,
    sounds: horizon_client::sound_events::SoundRegistry,
    armor: super::armor_stands::ArmorStandAssets,
    villager_hats: horizon_client::mobs::villager_hats::VillagerHats,
    waypoint_styles: horizon_client::locator_bar::WaypointStyles,
    language: lang::Language,
}

impl Game<'_> {
    fn vanilla_root(&self) -> PathBuf {
        if self.ui.vanilla_root.as_os_str().is_empty() {
            self.options.resources.clone()
        } else {
            self.ui.vanilla_root.clone()
        }
    }
    /// The packs the pack screen offers and the current selection
    /// (`PackSelectionScreen` makes its `PackSelectionModel` from them).
    pub(super) fn refresh_pack_lists(&mut self) {
        let infos = pack_infos(&self.vanilla_root());
        let o = &mut self.ui.options;
        o.pack_infos = infos;
        o.pack_selection = self.settings.resource_packs.clone();
        o.pack_screen = None;
    }
    /// `Minecraft.reloadResourcePacks`: rebuild the overlay and reload every
    /// client resource from it, as vanilla's reload listeners do: block
    /// models and sprites (every section is meshed again,
    /// `LevelRenderer.allChanged`), the shared atlas (GUI, item, entity,
    /// particle and font textures), item models, sounds, languages, GUI
    /// metadata and the cloud and world border textures.
    pub(super) fn reload_packs(&mut self) {
        // Pack reload updates tail-owned mob metadata. Keep menus responsive
        // until those resources exist, then apply the latest selection once.
        if self.rest_loader.is_some() || self.world_loader.is_some() {
            self.pending_pack_reload = true;
            return;
        }
        self.reload_selected_packs(false);
    }
    /// Reload from the selected packs and the server's (`server`: for the server's packs, which
    /// hear how it went), prepared on a worker while the game goes on (`poll_pack_reload` switches
    /// over): Android calls an app that does not take a touch for five seconds not responding,
    /// and a server's pack took about that long to load or to drop. One already preparing
    /// finishes first; this one follows it.
    pub(super) fn reload_selected_packs(&mut self, server: bool) {
        if self.pack_reload.is_some() {
            self.pending_pack_reload = true;
            return;
        }
        let vanilla = self.vanilla_root();
        if self.ui.vanilla_root.as_os_str().is_empty() {
            self.ui.vanilla_root = vanilla.clone();
        }
        let selected = self.settings.resource_packs.clone();
        let server_packs = self.server_packs.active.clone();
        let mods = self.settings.mods.enabled.clone();
        let dev_mods = self.options.dev_mods.clone();
        let assets = self.pack.assets.clone();
        let res = self.res.clone();
        let biomes = self.pack.context.biomes.clone();
        let light = self.pack.context.light.clone();
        let prepare = move || -> horizon_client::Result<PreparedPacks> {
            let root = startup_root(&vanilla, &selected, &server_packs)?;
            let root = super::mods::content_root(&root, &mods, &dev_mods);
            let render = Arc::new(assets.reload(&root)?);
            let gui = super::UiResources {
                atlas: HudAtlas::build_over(&root, &vanilla)?,
                panorama: Arc::new(horizon_client::gfx::panorama::Faces::load(&root)),
            };
            let pack =
                super::PackResources::load(&root, assets, render, res.facts(), biomes, light)?;
            Ok(PreparedPacks {
                textures: super::prepare_renderer_textures(&pack),
                sounds: horizon_client::sound_events::SoundRegistry::load(&root),
                armor: super::armor_stands::ArmorStandAssets::load(&root, None),
                villager_hats: horizon_client::mobs::villager_hats::VillagerHats::load(&root),
                waypoint_styles: horizon_client::locator_bar::WaypointStyles::load(&root),
                language: lang::Language::load(&[&root], &lang::code()),
                gui,
                pack,
                root,
            })
        };
        let spawned = std::thread::Builder::new()
            .name("pack-reload".into())
            .spawn(move || prepare().map_err(|e| e.to_string()));
        match spawned {
            Ok(handle) => {
                self.pack_reload = Some(PackReload {
                    handle,
                    server,
                    started: Instant::now(),
                });
            }
            Err(e) => self.pack_reload_done(server, Err(e.to_string())),
        }
    }
    /// The worker's reload, once prepared: the switch-over, then what waited for it.
    pub(super) fn poll_pack_reload(&mut self) {
        if !self
            .pack_reload
            .as_ref()
            .is_some_and(|r| r.handle.is_finished())
        {
            return;
        }
        let Some(PackReload {
            handle,
            server,
            started,
        }) = self.pack_reload.take()
        else {
            return;
        };
        let result = handle
            .join()
            .unwrap_or_else(|_| Err("the resource reload panicked".to_owned()))
            .and_then(|prepared| self.reload_resources(prepared).map_err(|e| e.to_string()));
        if result.is_ok() {
            log::info!(
                "Horizon: resources reloaded in {} ms",
                started.elapsed().as_millis()
            );
        }
        self.pack_reload_done(server, result);
        if std::mem::take(&mut self.pending_pack_reload) {
            self.reload_packs();
        }
    }
    fn pack_reload_done(&mut self, server: bool, result: Result<(), String>) {
        if server {
            if let Err(e) = &result {
                log::warn!("Horizon: server resource pack reload failed: {e}");
            }
            self.server_packs_reloaded(result.is_ok());
            return;
        }
        if let Err(e) = result {
            log::warn!("Horizon: resource reload failed: {e}");
            if self.screen != super::Screen::Options {
                self.open_options_at(
                    self.title.is_some(),
                    horizon_client::options_ui::Page::Packs,
                );
            }
            self.ui.options.resource_error = Some(e);
        }
    }
    fn reload_resources(&mut self, prepared: PreparedPacks) -> horizon_client::Result<()> {
        let PreparedPacks {
            root,
            mut gui,
            pack,
            textures,
            sounds,
            armor,
            villager_hats,
            waypoint_styles,
            language,
        } = prepared;
        let root = root.as_path();
        gui.atlas.font.uniform = self.settings.force_unicode;
        gui.atlas.font.japanese = self.settings.japanese_glyphs;
        // Keep the working GPU resources, but release its exclusive window surface.
        if let Some(renderer) = &mut self.renderer {
            renderer.release_surface();
        }
        let renderer = match self
            .window
            .clone()
            .map(|window| {
                super::build_renderer(
                    window,
                    self.options.offscreen,
                    &pack,
                    &gui,
                    self.renderer.as_ref(),
                    Some(textures),
                    self.settings.horizon.api,
                )
            })
            .transpose()
        {
            Ok(renderer) => renderer,
            Err(error) => {
                if let Some(renderer) = &mut self.renderer {
                    renderer.recreate_surface().map_err(|restore| {
                        format!(
                            "{error}; could not restore the previous rendering surface: {restore}"
                        )
                    })?;
                }
                return Err(error);
            }
        };
        // Everything loaded: switch over.
        self.waypoint_styles = waypoint_styles;
        self.armor_stand_assets.take_reloaded(armor);
        *self
            .res
            .mob_data()
            .armor_atlases
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) =
            self.armor_stand_assets.atlases.clone();
        self.random_tick_sections.clear();
        self.portals.sections.clear();
        self.options.resources = root.to_path_buf();
        let context = pack.context.clone();
        self.pack = pack;
        self.gui = gui;
        // The atlas's sprites moved: the particle engine re-reads every rect
        // and the packs' sprite sets, or every particle draws a stale region.
        self.particles
            .reload(root, &self.gui.atlas.sprites, &self.gui.atlas.translucent);
        horizon_client::ui_theme::reset_atlas();
        // Villager hats come from the pack's texture metadata (shared data, swapped in place).
        *self
            .res
            .mob_data()
            .villager_hats
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = villager_hats;
        horizon_client::ui::load_metadata(root);
        lang::set(language);
        self.refresh_remote_language();
        self.audio.reload_with(root, sounds);
        self.clear_lod();
        self.streaming.reload_resources(&mut self.world, context);
        self.pending_uploads.clear();
        // New GPU textures: a renderer built from the reloaded resources; the
        // sections are uploaded again as they are meshed.
        if let Some(renderer) = renderer {
            self.renderer = Some(renderer);
            self.apply_ui_settings();
        }
        self.ui.chat.rescale(&self.gui.atlas);
        Ok(())
    }
}
