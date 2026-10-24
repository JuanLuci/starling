//! Write Starling mods in Rust: Horizon's mod API (`horizon:mods`) and what Starling adds, in
//! its own WIT package (`starling:mods`, `api = "starling-0.1"` in the manifest).
//!
//! A mod is a type implementing [`Mod`], exported with [`export_mod!`] from
//! a `cdylib` crate built for `wasm32-unknown-unknown`;
//! `cargo horizon-mod build` turns it into the component the game loads.
//!
//! ```ignore
//! use starling_mod_sdk::{Mod, Registration, chat};
//!
//! struct Hello;
//! impl Mod for Hello {
//!     fn init() -> Result<(Self, Registration), String> {
//!         Ok((Hello, Registration::new().command("hello")))
//!     }
//!     fn command(&mut self, _name: &str, _args: &str) -> Result<Option<String>, String> {
//!         chat::show("Hello from a mod!");
//!         Ok(None)
//!     }
//! }
//! starling_mod_sdk::export_mod!(Hello);
//! ```
//!
//! Every interface but `log`, `types` and [`mobile`] is a capability: list the ones the mod uses in
//! `horizon-mod.toml` (`game-read`, `world-read`, `inventory-read`, `inventory-click` for
//! [`slots`], `creative`, `chat`, `chat-send`, `hud` for [`hud`] and [`draw`], `ui` for [`screens`]
//! and [`widgets`], `keys`, `input`, `sound`, `net`, `storage`).
//!
//! # Coming from Fabric
//!
//! | Fabric | Starling |
//! |---|---|
//! | `ClientModInitializer.onInitializeClient` | [`Mod::init`] |
//! | `ClientTickEvents.END_CLIENT_TICK` | [`Mod::tick`] ([`Registration::on_tick`]) |
//! | `HudRenderCallback` / `HudLayerRegistrationCallback` | [`Mod::hud_render`] |
//! | `HudElementRegistry.attachElementBefore/After`, `replaceElement` | [`Mod::hud_element`] ([`Registration::hud_element`]) |
//! | `ClientCommandRegistrationCallback` | [`Mod::command`] ([`Registration::command`]) |
//! | `ClientReceiveMessageEvents.ALLOW_GAME/MODIFY_GAME` | [`Mod::chat_received`] |
//! | `ClientSendMessageEvents.ALLOW_CHAT/MODIFY_CHAT` | [`Mod::chat_sending`] |
//! | `ScreenEvents.AFTER_INIT/afterRender/remove`, `ScreenMouseEvents`, `ScreenKeyboardEvents` | [`Mod::screen_opened`], [`Mod::screen_render`], [`Mod::screen_closed`], [`Mod::screen_mouse`], [`Mod::screen_key`] |
//! | A custom `Screen` | [`screens::open`], then the same hooks with `<mod id>:<name>` |
//! | Mod Menu's `ModMenuApi.getModConfigScreenFactory` | [`screens::set_config`] in [`Mod::init`] |
//! | `interactionManager.clickSlot(syncId, slot, button, SlotActionType, player)` | [`slots::click`] (capability `inventory-click`), the menu by vanilla's ids from [`slots::get_menu`] |
//! | `ItemGroups` / `ItemGroup.getDisplayStacks()`, the creative search | [`creative::get_tabs`], [`creative::get_items`] (capability `creative`) |
//! | `CreativeInventoryScreen.onMouseClick`, `ClientPlayerInteractionManager.clickCreativeStack` | [`creative::click`], [`creative::set_slot`], [`creative::set_carried`] |
//! | `MinecraftClient.setScreen` swapped for `CreativeInventoryScreen` | [`screens::replace`] |
//! | `InventoryScreen.drawEntity` | [`draw::player`] |
//! | `Screens.getButtons(screen).add(ButtonWidget...)` (in `ScreenEvents.AFTER_INIT`) | [`widgets::add_screen_button`] in [`Mod::init`]; a press is [`Mod::key`] with its id |
//! | Bedrock's touch buttons (a keybind's button on a phone) | [`widgets::add_touch_button`], [`widgets::style_control`] for the game's own |
//! | `ItemTooltipCallback` | [`Mod::tooltip`] |
//! | `KeyBindingHelper.registerKeyBinding` + `wasPressed` | [`Registration::keybind`], [`Mod::key`], [`keys::is_down`] |
//! | `options.forwardKey.isPressed()`, `KeyBinding.setPressed` (sprint, sneak) | [`keys::is_control_down`], [`keys::presses`], [`input::hold`] |
//! | `ClientPlayNetworking.registerGlobalReceiver` / `send` | [`Registration::channel`], [`Mod::payload`], [`net::send`] |
//! | `ClientPlayConnectionEvents.JOIN/DISCONNECT` | [`Mod::joined`], [`Mod::left`] |
//! | `UseItemCallback`, `UseBlockCallback`, `AttackEntityCallback`, `AttackBlockCallback` | [`Mod::use_item`], [`Mod::use_block`], [`Mod::attack_entity`], [`Mod::attack_block`] |
//! | `ClientPlayerBlockBreakEvents.AFTER` | [`Mod::block_broken`] |
//! | `MinecraftClient.player` / `.world` / `.crosshairTarget` | [`game::get_player`], [`level`], [`game::get_target`] |
//! | `DrawContext.fill/drawText/drawItem/drawGuiTexture/drawTooltip` | [`draw::fill`], [`draw::text`], [`draw::item`], [`draw::sprite`], [`draw::tooltip`] |

wit_bindgen::generate!({
    path: "wit",
    world: "starling-mod",
    pub_export_macro: true,
    export_macro_name: "__export_starling_mod",
    default_bindings_module: "starling_mod_sdk",
});

pub use exports::starling::mods::events::{
    ChatAction, Guest as __Guest, HudElement, HudPart, KeyEvent, Keybind, MouseAction, MouseEvent,
    Placement, Registration, RenderContext, SendAction,
};
pub use starling::mods::{
    chat, chat_send, creative, draw, game, hud, input, inventory, keys, level, log, mobile, net,
    screens,
    slots::{self, ClickKind},
    sound, storage,
    types::{self, BlockPos, Direction, GameMode, Hand, ItemStack},
    widgets::{self, Anchor, ScreenButton, Shape, TouchButton},
};

/// The game's calls into a mod. Only the hooks [`Registration`] asks for
/// are called; the others may stay as their defaults.
pub trait Mod: Sized + 'static {
    /// Once, after loading: the mod's state and the hooks it wants. An error
    /// keeps the mod off and is shown to the player.
    fn init() -> Result<(Self, Registration), String>;
    /// Every game tick, when [`Registration::on_tick`] asked.
    fn tick(&mut self, _tick: u64) {}
    /// Every rendered frame, when [`Registration::on_frame`] asked.
    fn frame(&mut self, _seconds: f32) {}
    /// `/name args` for a command the mod registered. `Ok(Some(text))` is
    /// shown as feedback, `Err(text)` in red.
    fn command(&mut self, _name: &str, _args: &str) -> Result<Option<String>, String> {
        Err("This command does nothing".into())
    }
    /// A chat line the player received ([`Registration::on_chat`]), as plain text and as a
    /// JSON text component.
    fn chat_received(&mut self, _plain: &str, _component: &str) -> ChatAction {
        ChatAction::Keep
    }
    /// A line or `/command` the player is about to send ([`Registration::on_chat_send`]).
    fn chat_sending(&mut self, _message: &str) -> SendAction {
        SendAction::Send
    }
    /// Over the HUD, every frame ([`Registration::on_hud`]): draw with [`draw`].
    fn hud_render(&mut self, _context: RenderContext) {}
    /// One of the mod's [`Registration::hud_element`]s, at its place in the HUD.
    fn hud_element(&mut self, _id: &str, _context: RenderContext) {}
    /// A screen opened ([`Registration::on_screens`], or the mod's own screen).
    fn screen_opened(&mut self, _screen: &str) {}
    fn screen_closed(&mut self, _screen: &str) {}
    /// After the screen drew; a mod's own screen draws everything here.
    fn screen_render(&mut self, _screen: &str, _context: RenderContext) {}
    /// True when the mod used the event (the screen does not get it).
    fn screen_mouse(&mut self, _screen: &str, _event: MouseEvent) -> bool {
        false
    }
    fn screen_key(&mut self, _screen: &str, _event: &KeyEvent) -> bool {
        false
    }
    /// Lines added under an item's tooltip ([`Registration::on_tooltips`]).
    fn tooltip(&mut self, _stack: &ItemStack) -> Vec<String> {
        Vec::new()
    }
    /// One of the mod's [`Registration::keybind`]s pressed or released.
    fn key(&mut self, _id: &str, _pressed: bool) {}
    /// A payload on one of the mod's [`Registration::channel`]s.
    fn payload(&mut self, _channel: &str, _data: &[u8]) {}
    /// The player entered a world ([`Registration::on_connection`]): single player (None) or
    /// a server's address.
    fn joined(&mut self, _server: Option<&str>) {}
    fn left(&mut self) {}
    fn dimension_changed(&mut self, _dimension: &str) {}
    /// [`Registration::on_interaction`]: false cancels the action.
    fn use_item(&mut self, _hand: Hand) -> bool {
        true
    }
    fn use_block(&mut self, _pos: BlockPos, _face: Direction, _hand: Hand) -> bool {
        true
    }
    fn attack_entity(&mut self, _entity: i32) -> bool {
        true
    }
    fn attack_block(&mut self, _pos: BlockPos, _face: Direction) -> bool {
        true
    }
    fn block_broken(&mut self, _pos: BlockPos, _block: &str) {}
    /// The player lost health ([`Registration::on_damage`]).
    fn player_hurt(&mut self, _amount: f32) {}
}

impl Registration {
    /// No commands and no hooks.
    pub fn new() -> Self {
        Self {
            commands: Vec::new(),
            tick: false,
            frame: false,
            chat: false,
            chat_send: false,
            hud: false,
            hud_elements: Vec::new(),
            screens: false,
            tooltips: false,
            keybinds: Vec::new(),
            channels: Vec::new(),
            connection: false,
            interaction: false,
            damage: false,
        }
    }
    /// Adds `/name`.
    pub fn command(mut self, name: &str) -> Self {
        self.commands.push(name.into());
        self
    }
    pub fn on_tick(mut self) -> Self {
        self.tick = true;
        self
    }
    pub fn on_frame(mut self) -> Self {
        self.frame = true;
        self
    }
    pub fn on_chat(mut self) -> Self {
        self.chat = true;
        self
    }
    pub fn on_chat_send(mut self) -> Self {
        self.chat_send = true;
        self
    }
    pub fn on_hud(mut self) -> Self {
        self.hud = true;
        self
    }
    /// Draws [`Mod::hud_element`] `id` before, after or instead of a vanilla HUD part.
    pub fn hud_element(mut self, id: &str, part: HudPart, placement: Placement) -> Self {
        self.hud_elements.push(HudElement {
            id: id.into(),
            part,
            placement,
        });
        self
    }
    pub fn on_screens(mut self) -> Self {
        self.screens = true;
        self
    }
    pub fn on_tooltips(mut self) -> Self {
        self.tooltips = true;
        self
    }
    /// A keybind `id` named `name` on `key` by default (`key.keyboard.r`).
    pub fn keybind(mut self, id: &str, name: &str, key: &str) -> Self {
        self.keybinds.push(Keybind {
            id: id.into(),
            name: name.into(),
            key: key.into(),
        });
        self
    }
    /// A payload channel (`namespace:path`) the mod sends and receives on.
    pub fn channel(mut self, channel: &str) -> Self {
        self.channels.push(channel.into());
        self
    }
    pub fn on_connection(mut self) -> Self {
        self.connection = true;
        self
    }
    pub fn on_interaction(mut self) -> Self {
        self.interaction = true;
        self
    }
    pub fn on_damage(mut self) -> Self {
        self.damage = true;
        self
    }
}
impl Default for Registration {
    fn default() -> Self {
        Self::new()
    }
}

/// `log::write` at info level.
pub fn info(message: &str) {
    log::write(log::Level::Info, message);
}
/// `log::write` at warn level.
pub fn warn(message: &str) {
    log::write(log::Level::Warn, message);
}

/// Exports a [`Mod`] as the component's `events` interface. The state lives
/// in a thread local: a mod instance is single-threaded.
#[macro_export]
macro_rules! export_mod {
    ($t:ty) => {
        const _: () = {
            use ::std::cell::RefCell;
            ::std::thread_local! {
                static STATE: RefCell<Option<$t>> = const { RefCell::new(None) };
            }
            fn with<R>(default: R, f: impl FnOnce(&mut $t) -> R) -> R {
                STATE.with(|s| match s.borrow_mut().as_mut() {
                    Some(m) => f(m),
                    None => default,
                })
            }
            struct Export;
            impl $crate::__Guest for Export {
                fn init() -> Result<$crate::Registration, String> {
                    let (m, registration) = <$t as $crate::Mod>::init()?;
                    STATE.with(|s| *s.borrow_mut() = Some(m));
                    Ok(registration)
                }
                fn tick(tick: u64) {
                    with((), |m| $crate::Mod::tick(m, tick))
                }
                fn frame(seconds: f32) {
                    with((), |m| $crate::Mod::frame(m, seconds))
                }
                fn command(name: String, args: String) -> Result<Option<String>, String> {
                    with(Err("The mod is not running".to_string()), |m| {
                        $crate::Mod::command(m, &name, &args)
                    })
                }
                fn chat_received(plain: String, component: String) -> $crate::ChatAction {
                    with($crate::ChatAction::Keep, |m| {
                        $crate::Mod::chat_received(m, &plain, &component)
                    })
                }
                fn chat_sending(message: String) -> $crate::SendAction {
                    with($crate::SendAction::Send, |m| {
                        $crate::Mod::chat_sending(m, &message)
                    })
                }
                fn hud_render(context: $crate::RenderContext) {
                    with((), |m| $crate::Mod::hud_render(m, context))
                }
                fn render_hud_element(id: String, context: $crate::RenderContext) {
                    with((), |m| $crate::Mod::hud_element(m, &id, context))
                }
                fn screen_opened(screen: String) {
                    with((), |m| $crate::Mod::screen_opened(m, &screen))
                }
                fn screen_closed(screen: String) {
                    with((), |m| $crate::Mod::screen_closed(m, &screen))
                }
                fn screen_render(screen: String, context: $crate::RenderContext) {
                    with((), |m| $crate::Mod::screen_render(m, &screen, context))
                }
                fn screen_mouse(screen: String, event: $crate::MouseEvent) -> bool {
                    with(false, |m| $crate::Mod::screen_mouse(m, &screen, event))
                }
                fn screen_key(screen: String, event: $crate::KeyEvent) -> bool {
                    with(false, |m| $crate::Mod::screen_key(m, &screen, &event))
                }
                fn tooltip(stack: $crate::ItemStack) -> Vec<String> {
                    with(Vec::new(), |m| $crate::Mod::tooltip(m, &stack))
                }
                fn key(id: String, pressed: bool) {
                    with((), |m| $crate::Mod::key(m, &id, pressed))
                }
                fn payload(channel: String, data: Vec<u8>) {
                    with((), |m| $crate::Mod::payload(m, &channel, &data))
                }
                fn joined(server: Option<String>) {
                    with((), |m| $crate::Mod::joined(m, server.as_deref()))
                }
                fn left() {
                    with((), |m| $crate::Mod::left(m))
                }
                fn dimension_changed(dimension: String) {
                    with((), |m| $crate::Mod::dimension_changed(m, &dimension))
                }
                fn use_item(hand: $crate::Hand) -> bool {
                    with(true, |m| $crate::Mod::use_item(m, hand))
                }
                fn use_block(
                    pos: $crate::BlockPos,
                    face: $crate::Direction,
                    hand: $crate::Hand,
                ) -> bool {
                    with(true, |m| $crate::Mod::use_block(m, pos, face, hand))
                }
                fn attack_entity(entity: i32) -> bool {
                    with(true, |m| $crate::Mod::attack_entity(m, entity))
                }
                fn attack_block(pos: $crate::BlockPos, face: $crate::Direction) -> bool {
                    with(true, |m| $crate::Mod::attack_block(m, pos, face))
                }
                fn block_broken(pos: $crate::BlockPos, block: String) {
                    with((), |m| $crate::Mod::block_broken(m, pos, &block))
                }
                fn player_hurt(amount: f32) {
                    with((), |m| $crate::Mod::player_hurt(m, amount))
                }
            }
            $crate::__export_starling_mod!(Export);
        };
    };
}
