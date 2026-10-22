//! A mod for the runtime's tests: `/spin` loops forever, `/panic` panics,
//! `/grow` asks for a gigabyte, `/hello` chats, `/save` and `/load` exercise
//! capability `storage` on a fixed key, `/save_key <key> <text>` exercises
//! it on an arbitrary key (for the quota and key-count limits); `tick` and
//! `frame` do nothing, to measure the cost of a call.
//!
//! API 0.3's hooks: it draws over the HUD and after the hotbar, opens `/open` its screen
//! `test_guest:menu` (a click inside 10..50 is its own), hides chat lines with "secret",
//! rewrites "swap", cancels `/blocked`, adds a tooltip line, sends `/send <text>` as chat,
//! echoes payloads on `test:echo` and refuses to use the off hand.
use starling_mod_sdk::{
    ChatAction, Hand, HudPart, ItemStack, Mod, MouseAction, MouseEvent, Placement, Registration,
    RenderContext, SendAction, chat, chat_send, draw, game, net, screens, storage,
};

struct Guest {
    ticks: u64,
    keys: Vec<String>,
}

impl Mod for Guest {
    fn init() -> Result<(Self, Registration), String> {
        let hooks = [
            "spin", "panic", "grow", "hello", "save", "load", "save_key", "open", "send",
        ]
        .into_iter()
        .fold(Registration::new(), Registration::command)
        .on_tick()
        .on_frame()
        .on_hud()
        .hud_element("after_hotbar", HudPart::Hotbar, Placement::After)
        .hud_element("no_food", HudPart::Food, Placement::Replace)
        .on_chat()
        .on_chat_send()
        .on_tooltips()
        .on_interaction()
        .keybind("zoom", "Zoom", "key.keyboard.c")
        .channel("test:echo")
        .channel("Bad Channel");
        Ok((
            Guest {
                ticks: 0,
                keys: Vec::new(),
            },
            hooks,
        ))
    }
    fn tick(&mut self, _tick: u64) {
        self.ticks += 1;
    }
    fn command(&mut self, name: &str, args: &str) -> Result<Option<String>, String> {
        match name {
            "spin" => {
                let mut n = 0u64;
                while std::hint::black_box(n) != u64::MAX {
                    n = n.wrapping_add(2);
                }
                Ok(None)
            }
            "panic" => panic!("asked to"),
            "grow" => {
                let big = std::hint::black_box(vec![1u8; 1 << 30]);
                Ok(Some(format!("{}", big.len())))
            }
            "save" => {
                storage::set("note", args.as_bytes())?;
                Ok(Some("saved".into()))
            }
            "load" => {
                let text =
                    storage::get("note").map(|bytes| String::from_utf8_lossy(&bytes).into_owned());
                chat::show(text.as_deref().unwrap_or("(empty)"));
                Ok(None)
            }
            "save_key" => {
                let (key, text) = args
                    .split_once(' ')
                    .ok_or("Usage: /save_key <key> <text>")?;
                storage::set(key, text.as_bytes())?;
                Ok(Some("saved".into()))
            }
            "open" => {
                screens::open("menu");
                Ok(None)
            }
            "send" => {
                chat_send::send(args)?;
                Ok(Some(format!("keys {}", self.keys.join(","))))
            }
            _ => {
                chat::show(&format!("hello {args} after {} ticks", self.ticks));
                Ok(None)
            }
        }
    }
    fn hud_render(&mut self, context: RenderContext) {
        draw::fill(1.0, 2.0, 3.0, 4.0, 0xFF0000);
        let width = draw::text(5.0, 6.0, "hi", 0xFFFFFF, true);
        draw::fill(width, 0.0, context.width as f32, 1.0, 0x00FF00);
    }
    fn hud_element(&mut self, id: &str, _context: RenderContext) {
        draw::sprite(0.0, 0.0, 9.0, 9.0, &format!("hud/{id}"));
    }
    fn screen_render(&mut self, screen: &str, context: RenderContext) {
        draw::panel(0.0, 0.0, context.width as f32, context.height as f32);
        draw::button(10.0, 10.0, 40.0, 20.0, screen, context.mouse_x < 50.0, true);
    }
    fn screen_mouse(&mut self, _screen: &str, event: MouseEvent) -> bool {
        let inside = (10.0..50.0).contains(&event.x) && (10.0..30.0).contains(&event.y);
        if inside && event.action == MouseAction::Press {
            screens::close();
        }
        inside
    }
    fn chat_received(&mut self, plain: &str, _component: &str) -> ChatAction {
        if plain.contains("secret") {
            ChatAction::Hide
        } else if plain.contains("swap") {
            ChatAction::Replace(plain.replace("swap", "swapped"))
        } else {
            ChatAction::Keep
        }
    }
    fn chat_sending(&mut self, message: &str) -> SendAction {
        match message {
            "/blocked" => SendAction::Cancel,
            "lower" => SendAction::Replace("LOWER".into()),
            _ => SendAction::Send,
        }
    }
    fn tooltip(&mut self, stack: &ItemStack) -> Vec<String> {
        let health = game::get_player().map_or(0.0, |p| p.health);
        vec![format!("{} x{} at {health}", stack.id, stack.count)]
    }
    fn key(&mut self, id: &str, pressed: bool) {
        self.keys.push(format!("{id}:{pressed}"));
    }
    fn payload(&mut self, channel: &str, data: &[u8]) {
        let _ = net::send(channel, data);
    }
    fn use_item(&mut self, hand: Hand) -> bool {
        hand == Hand::MainHand
    }
}

starling_mod_sdk::export_mod!(Guest);
