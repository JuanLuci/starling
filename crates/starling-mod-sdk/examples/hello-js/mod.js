// A Starling mod in JavaScript, shipped as it is (beside horizon-mod.toml): Starling's shared
// engine runs it. /hello_js (or its button on the pause menu) says where you are, the HUD shows
// the FPS, and tooltips name the item's id.
import { write } from "starling:mods/log@0.1.0";
import { getPlayer, getFps } from "starling:mods/game@0.1.0";
import { show } from "starling:mods/chat@0.1.0";
import { text } from "starling:mods/draw@0.1.0";
import { addScreenButton } from "starling:mods/widgets@0.1.0";

// Says where the player is. A button's onPress must not throw: like any hook, a throw stops the
// mod.
function hello() {
  const p = getPlayer();
  show(p ? `Hello from JS at ${p.x.toFixed(1)} ${p.y.toFixed(1)} ${p.z.toFixed(1)}` : "Not in a world");
}

export const events = {
  init() {
    write("info", "hello from JavaScript");
    // As Fabric's Screens.getButtons(screen).add(ButtonWidget.builder(text, onPress)...): the
    // fields left out take their usual values (20 tall, active, no icon).
    addScreenButton(
      { id: "hello", screen: "pause", anchor: "bottom-left", x: 4, y: -4, width: 70,
        label: "Say hello", icon: "item/cookie", tooltip: ["Where am I?"] },
      () => hello(),
    );
    return {
      commands: ["hello_js"], tick: false, frame: false, chat: false, chatSend: false,
      hud: true, hudElements: [], screens: false, tooltips: true, keybinds: [], channels: [],
      connection: false, interaction: false, damage: false,
    };
  },
  tick(_tick) {},
  frame(_seconds) {},
  command(_name, _args) {
    hello();
    return undefined;
  },
  chatReceived(_plain, _component) { return { tag: "keep" }; },
  chatSending(_message) { return { tag: "send" }; },
  hudRender(context) { text(4, context.height - 60, `JS ${getFps()} fps`, 0xFFFF55, true); },
  renderHudElement(_id, _context) {},
  screenOpened(_screen) {},
  screenClosed(_screen) {},
  screenRender(_screen, _context) {},
  screenMouse(_screen, _event) { return false; },
  screenKey(_screen, _event) { return false; },
  tooltip(stack) { return [`§8${stack.id}`]; },
  key(_id, _pressed) {},
  payload(_channel, _data) {},
  joined(_server) {},
  left() {},
  dimensionChanged(_dimension) {},
  useItem(_hand) { return true; },
  useBlock(_pos, _face, _hand) { return true; },
  attackEntity(_entity) { return true; },
  attackBlock(_pos, _face) { return true; },
  blockBroken(_pos, _block) {},
  playerHurt(_amount) {},
};
