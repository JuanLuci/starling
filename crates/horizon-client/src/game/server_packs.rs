//! A server's resource packs (`ServerPackManager`, `DownloadedPackSource`): the prompt the
//! server's choice in the server list leaves to the player, the download (checked against the
//! SHA-1 the server gives, kept for the next visit), the packs over the player's own, and each
//! step answered to the server, which waits for them.
use super::{Game, Screen, mods::rich_text};
use horizon_client::{
    hud::{HudBatch, rgba},
    screen_draw::{self, Measure},
    text::Text,
};
use horizon_protocol::packets::configuration::ResourcePackPush;
use std::{
    collections::VecDeque,
    io::Read,
    path::PathBuf,
    sync::mpsc::{Receiver, channel},
};
use winit::{
    event::{ElementState, MouseButton, WindowEvent},
    keyboard::{KeyCode, PhysicalKey},
};

/// ServerboundResourcePackPacket.Action.
const LOADED: i32 = 0;
const DECLINED: i32 = 1;
const FAILED_DOWNLOAD: i32 = 2;
const ACCEPTED: i32 = 3;
const DOWNLOADED: i32 = 4;
const INVALID_URL: i32 = 5;
const FAILED_RELOAD: i32 = 6;
/// `DownloadQueue`'s limit on a pack's size.
const MAX_BYTES: u64 = 250 << 20;

/// A download's end: the pack's file, or the answer for the server and why.
type Downloaded = Result<PathBuf, (i32, String)>;

#[derive(Default)]
pub(super) struct ServerPacks {
    /// The packs in use, bottom to top.
    applied: Vec<(u128, PathBuf)>,
    /// Pushes waiting for the player's answer; the prompt shows the first.
    prompts: VecDeque<ResourcePackPush>,
    /// Downloads under way: the pack, whether the server requires it, and the result.
    downloads: Vec<(u128, bool, Receiver<Downloaded>)>,
    /// Packs downloaded since the last reload, to answer once it is done.
    loading: Vec<(u128, bool)>,
}
impl ServerPacks {
    pub(super) fn applied(&self) -> Vec<(u128, PathBuf)> {
        self.applied.clone()
    }
    pub(super) fn prompting(&self) -> bool {
        !self.prompts.is_empty()
    }
}

/// Where downloaded packs are kept, by their SHA-1 (the private folder on Android).
fn downloads_dir() -> PathBuf {
    horizon_client::logging::cache_dir().join("server-packs")
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// `DownloadQueue.runDownload`: the pack at `url`, checked against `hash` when there is one.
fn download(url: &str, hash: &str, player: &str, uuid: &str) -> Downloaded {
    let hash = hash.to_ascii_lowercase();
    let well_formed = hash.len() == 40 && hash.bytes().all(|b| b.is_ascii_hexdigit());
    let dir = downloads_dir();
    if well_formed {
        let kept = dir.join(format!("{hash}.zip"));
        if let Ok(bytes) = std::fs::read(&kept)
            && hex(&horizon_client::skins::sha1(&bytes)) == hash
        {
            return Ok(kept);
        }
    }
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err((INVALID_URL, format!("not a web address: {url}")));
    }
    let version = horizon_core::version::VERSION_ID;
    let response = ureq::AgentBuilder::new()
        .timeout_connect(std::time::Duration::from_secs(15))
        .timeout_read(std::time::Duration::from_secs(30))
        .user_agent(&format!("Minecraft Java/{version}"))
        .build()
        .get(url)
        // HttpUtil.downloadFile's headers: some pack hosts check them.
        .set("X-Minecraft-Username", player)
        .set("X-Minecraft-UUID", uuid)
        .set("X-Minecraft-Version", version)
        .set("X-Minecraft-Version-ID", version)
        .set(
            "X-Minecraft-Pack-Format",
            &horizon_client::packs::RESOURCE_FORMAT.0.to_string(),
        )
        .call()
        .map_err(|e| (FAILED_DOWNLOAD, e.to_string()))?;
    let mut bytes = Vec::new();
    response
        .into_reader()
        .take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| (FAILED_DOWNLOAD, e.to_string()))?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err((FAILED_DOWNLOAD, format!("over {} MiB", MAX_BYTES >> 20)));
    }
    let got = hex(&horizon_client::skins::sha1(&bytes));
    if well_formed && got != hash {
        return Err((
            FAILED_DOWNLOAD,
            format!("its SHA-1 is {got}, the server said {hash}"),
        ));
    }
    std::fs::create_dir_all(&dir).map_err(|e| (FAILED_DOWNLOAD, e.to_string()))?;
    let path = dir.join(format!("{got}.zip"));
    let part = dir.join(format!("{got}.part"));
    std::fs::write(&part, &bytes)
        .and_then(|()| std::fs::rename(&part, &path))
        .map_err(|e| (FAILED_DOWNLOAD, e.to_string()))?;
    Ok(path)
}

impl Game<'_> {
    fn pack_response(&self, id: u128, action: i32) {
        if let Some(r) = &self.remote {
            r.session.pack_response(id, action);
        }
    }
    /// The server's choice in the server list (`ServerData.ResourcePackStatus`): Some(true)
    /// enabled, Some(false) disabled, None prompt.
    fn pack_policy(&self) -> Option<bool> {
        let address = &self.remote.as_ref()?.session.address;
        let list =
            horizon_protocol::servers::load(&std::env::current_dir().ok()?.join("servers.dat"));
        list.iter()
            .find(|s| &s.ip == address)
            .and_then(|s| s.accept_textures)
    }
    /// Remembers the player's answer for this server (`ServerData.setResourcePackStatus`).
    fn remember_pack_policy(&self, accept: bool) {
        let Some(address) = self.remote.as_ref().map(|r| r.session.address.clone()) else {
            return;
        };
        let Ok(path) = std::env::current_dir().map(|d| d.join("servers.dat")) else {
            return;
        };
        let mut list = horizon_protocol::servers::load(&path);
        if let Some(entry) = list.iter_mut().find(|s| s.ip == address) {
            entry.accept_textures = Some(accept);
            if let Err(e) = horizon_protocol::servers::save(&path, &list) {
                log::warn!("Horizon: could not save servers.dat: {e}");
            }
        }
    }
    /// ClientCommonPacketListenerImpl.handleResourcePackPush.
    pub(super) fn server_pack_push(&mut self, push: ResourcePackPush) {
        log::info!(
            "Horizon: the server {} a resource pack: {}",
            if push.required { "requires" } else { "offers" },
            push.url
        );
        match self.pack_policy() {
            Some(true) => self.start_pack_download(push),
            Some(false) => self.decline_pack(&push),
            None => {
                self.server_packs.prompts.retain(|p| p.id != push.id);
                self.server_packs.prompts.push_back(push);
                self.capture(false);
            }
        }
    }
    /// The prompt's answer for the first push waiting.
    pub(super) fn answer_pack_prompt(&mut self, accept: bool) {
        let Some(push) = self.server_packs.prompts.pop_front() else {
            return;
        };
        self.remember_pack_policy(accept);
        if accept {
            self.start_pack_download(push);
        } else {
            self.decline_pack(&push);
        }
        if self.server_packs.prompts.is_empty()
            && self.screen == Screen::Playing
            && self.title.is_none()
            && self.player.is_some()
        {
            self.capture(true);
        }
    }
    fn decline_pack(&mut self, push: &ResourcePackPush) {
        self.pack_response(push.id, DECLINED);
        if push.required {
            // ServerPackManager: a required pack declined leaves the server.
            let reason = self.translate(
                "multiplayer.requiredTexturePrompt.disconnect",
                "Server requires a custom resource pack",
            );
            if let Err(e) = self.leave_server(Some(reason)) {
                log::warn!("Horizon: {e}");
            }
        }
    }
    fn start_pack_download(&mut self, push: ResourcePackPush) {
        self.pack_response(push.id, ACCEPTED);
        let (player, uuid) = self.remote.as_ref().map_or_else(Default::default, |r| {
            let h = format!("{:032x}", r.profile);
            (r.session.name.clone(), h)
        });
        let (tx, rx) = channel();
        let (url, hash) = (push.url.clone(), push.hash.clone());
        let started = std::thread::Builder::new()
            .name("horizon-server-pack".into())
            .spawn(move || {
                let _ = tx.send(download(&url, &hash, &player, &uuid));
            });
        if started.is_err() {
            self.pack_response(push.id, FAILED_DOWNLOAD);
            return;
        }
        self.server_packs
            .downloads
            .retain(|(id, _, _)| *id != push.id);
        self.server_packs
            .downloads
            .push((push.id, push.required, rx));
    }
    /// ClientboundResourcePackPopPacket: one pack, or every one.
    pub(super) fn server_pack_pop(&mut self, id: Option<u128>) {
        let packs = &mut self.server_packs;
        let before = packs.applied.len();
        packs.applied.retain(|(p, _)| id.is_some_and(|id| id != *p));
        packs.prompts.retain(|p| id.is_some_and(|id| id != p.id));
        packs
            .downloads
            .retain(|(p, _, _)| id.is_some_and(|id| id != *p));
        if packs.applied.len() != before {
            self.reload_packs();
        }
    }
    /// Leaving the server: its packs go, and the player's own come back.
    pub(super) fn drop_server_packs(&mut self) {
        let had = !self.server_packs.applied.is_empty();
        self.server_packs = Default::default();
        if had {
            self.reload_packs();
        }
    }
    /// Once a frame: finished downloads, then one reload for the packs they brought.
    pub(super) fn poll_server_packs(&mut self) {
        if self.server_packs.downloads.is_empty() && self.server_packs.loading.is_empty() {
            return;
        }
        let mut done = Vec::new();
        self.server_packs
            .downloads
            .retain(|(id, required, rx)| match rx.try_recv() {
                Ok(result) => {
                    done.push((*id, *required, result));
                    false
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => true,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    done.push((*id, *required, Err((FAILED_DOWNLOAD, "stopped".into()))));
                    false
                }
            });
        for (id, required, result) in done {
            match result {
                Ok(path) => {
                    self.pack_response(id, DOWNLOADED);
                    let packs = &mut self.server_packs;
                    packs.applied.retain(|(p, _)| *p != id);
                    packs.applied.push((id, path));
                    packs.loading.push((id, required));
                }
                Err((action, why)) => {
                    log::warn!("Horizon: the server's resource pack failed: {why}");
                    self.pack_response(id, action);
                    if required {
                        let reason = self.translate(
                            "multiplayer.requiredTexturePrompt.disconnect",
                            "Server requires a custom resource pack",
                        );
                        if let Err(e) = self.leave_server(Some(reason)) {
                            log::warn!("Horizon: {e}");
                        }
                        return;
                    }
                }
            }
        }
        // Every pack of this push in hand: one reload for them all.
        if self.server_packs.downloads.is_empty() && !self.server_packs.loading.is_empty() {
            let loading = std::mem::take(&mut self.server_packs.loading);
            let result = self.try_reload_packs();
            for (id, required) in loading {
                match &result {
                    Ok(()) => self.pack_response(id, LOADED),
                    Err(e) => {
                        log::warn!("Horizon: the server's resource pack did not load: {e}");
                        self.pack_response(id, FAILED_RELOAD);
                        if required {
                            self.server_packs.applied.retain(|(p, _)| *p != id);
                        }
                    }
                }
            }
        }
    }
    /// The prompt's two buttons: (x, y, w, h).
    fn pack_prompt_buttons(batch: &HudBatch) -> [[f32; 4]; 2] {
        let (cx, y) = (
            (batch.width / 2.0).floor(),
            (batch.height / 6.0 + 96.0).floor(),
        );
        [[cx - 155.0, y, 150.0, 20.0], [cx + 5.0, y, 150.0, 20.0]]
    }
    /// ConfirmScreen of `ServerPackManager`'s prompt, over everything.
    pub(super) fn draw_pack_prompt(&mut self, batch: &mut HudBatch) {
        let Some(push) = self.server_packs.prompts.front() else {
            return;
        };
        let atlas = &self.gui.atlas;
        let (w, h) = (batch.width, batch.height);
        batch.fill_gradient(
            atlas,
            (0.0, 0.0, w, h),
            rgba(16, 16, 16, 220),
            rgba(16, 16, 16, 235),
        );
        let (title, line2, yes, no) = if push.required {
            (
                Text::translate("multiplayer.requiredTexturePrompt.line1", &[]),
                Text::translate("multiplayer.requiredTexturePrompt.line2", &[]),
                Text::translate("gui.proceed", &[]),
                Text::translate("menu.disconnect", &[]),
            )
        } else {
            (
                Text::translate("multiplayer.texturePrompt.line1", &[]),
                Text::translate("multiplayer.texturePrompt.line2", &[]),
                Text::translate("gui.yes", &[]),
                Text::translate("gui.no", &[]),
            )
        };
        let mut lines = vec![title, line2];
        if let Some(prompt) = &push.prompt {
            let json = horizon_client::scoreboard::tag_to_json(prompt);
            lines.push(rich_text(&json.to_string()));
        }
        let m = Measure(&atlas.font);
        let mut y = 70.0;
        for line in &lines {
            let tw = m.width(line) as f32;
            batch.text_styled(atlas, line, ((w - tw) / 2.0).floor(), y, u32::MAX, true);
            y += 12.0;
        }
        let mut draws = Vec::new();
        for (b, label) in Self::pack_prompt_buttons(batch).iter().zip([yes, no]) {
            let hovered = self.cursor.0 >= b[0]
                && self.cursor.0 < b[0] + b[2]
                && self.cursor.1 >= b[1]
                && self.cursor.1 < b[1] + b[3];
            screen_draw::button(
                &mut draws,
                &m,
                &label,
                b[0] as i32,
                b[1] as i32,
                b[2] as i32,
                b[3] as i32,
                true,
                hovered,
            );
        }
        screen_draw::paint(batch, atlas, &draws);
    }
    /// The prompt takes the mouse and Escape while it shows. True when it used the event.
    pub(super) fn pack_prompt_window_event(&mut self, event: &WindowEvent) -> bool {
        if !self.server_packs.prompting() {
            return false;
        }
        match event {
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Left,
                ..
            } => {
                let Some(r) = &self.renderer else {
                    return true;
                };
                let batch = HudBatch::new([r.config.width, r.config.height]);
                let (x, y) = self.cursor;
                let hit = Self::pack_prompt_buttons(&batch)
                    .iter()
                    .position(|b| x >= b[0] && x < b[0] + b[2] && y >= b[1] && y < b[1] + b[3]);
                if let Some(i) = hit {
                    self.audio.play_button_click();
                    self.answer_pack_prompt(i == 0);
                }
                true
            }
            WindowEvent::KeyboardInput { event: k, .. } => {
                if k.state == ElementState::Pressed
                    && k.physical_key == PhysicalKey::Code(KeyCode::Escape)
                {
                    self.answer_pack_prompt(false);
                }
                true
            }
            WindowEvent::MouseInput { .. } | WindowEvent::MouseWheel { .. } => true,
            _ => false,
        }
    }
}
