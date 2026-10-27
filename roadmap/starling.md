# Starling roadmap

What is being done on Starling, the Android port of Horizon, what comes next and what waits. There are
no dates: things are Now, Next or Later. Each item says in plain words what you will be able to do, with
the technical picture under it. The game itself is Horizon's: its own roadmap is [ROADMAP.md](../ROADMAP.md).

## At a glance

**Being done now**

- **Shader packs on a phone**: Complementary and MakeUp running smoothly on a phone, picked on the
  Shader Packs page.
- **Servers with plugins**: The custom items, armour, models, menus and buttons of plugin servers
  (ItemsAdder, Nexo, ModelEngine, BetterModel, dialogs) look and work as on Java.
- **Settings that make sense on a phone**: Every option does what it says, and the ones a phone can't
  use are out of the way.

**Next**: a pack's own options in the game; secure chat for servers that require it; physical
controllers on Android; preparing the game files on the phone, without a computer; Open to LAN and
invites on a phone; the first release.

**Later**: a store release; more of the game open to mods; a phone-sized mod browser.

## Now

### Shader packs on a phone

You play with Complementary or MakeUp on your phone at a frame rate you can play at, and choose the pack
and its quality in the game.

> Horizon's shader-pack translator (crates/horizon-shaders) runs Iris and OptiFine packs as they are,
> GLSL translated to WGSL at load time; nothing is hard-coded per pack. On an Adreno 740 at half
> resolution Complementary's LOW profile draws in about 16 to 21 ms of GPU time (45 to 55 fps) with the
> experimental depth pass for solid terrain (`HORIZON_SHADER_PACK_PREPASS`), which lets the tiler drop
> hidden grass and leaves before the pack's fragment stage runs. Done: the Shader Packs page (Video
> Settings: the pack and its profile, saved in horizon-graphics.txt), option values and profiles,
> constant texture matrices, a depth pass that keeps no more than the pack's own alpha cut, resolution
> down to 25%, outputs kept finite (Adreno made NaNs that turned Complementary black for good after a
> pond was flooded), `cameraPositionInt`/`Fract`. Left: checking on the phone that the sun glare, the
> water's dark band and the shadows that flicker at 25% went with the NaNs; the pack's own options
> screen (`HORIZON_SHADER_OPTIONS` for now) and light defaults on phones; compiling packs off the main
> thread with a cache (a stall of a few seconds on joining a world or changing the pack);
> `cloudHeight` and the other uniforms still read as zero; the depth pass on by default once it is
> checked on more packs.

### Servers with plugins

On a server with plugins you see its custom items, armour and models, click its chat buttons, open its
menus and dialogs, and stay on it without the anticheat pulling you back.

> Done: dialogs; server resource packs reloaded on a worker (no "app not responding" on join);
> ItemsAdder's custom armour (its leather-atlas shader trick emulated, custom_armor.rs) and armour from
> any namespace a stack's own `equippable` names (Nexo, Oraxen, ItemsAdder's modern mode); chat click
> events (`run_command`, `open_url` with vanilla's confirmation, `show_dialog`, `custom`); interaction
> entities that can be hit and used; display entities that ride their mob or the player (ModelEngine and
> BetterModel models, cosmetics, nameplates) and interpolate their transformation and moves as vanilla
> does; item cooldowns, forced looks and custom chat completions;
> the player's real language, chat visibility and main hand sent to the server; Free Direction, D-pad
> and Analog movement chosen per server, tested against GrimAC; text `shadow_color` and a glyph region
> that starts again when full (HUD and menu plugins); block displays drawn from their block state, and
> displays' brightness. Left, most important first: secure chat (servers that enforce signed chat),
> glowing outlines, the components
> still dropped from items (custom tooltips, food, gliders), `object` text (sprites and heads in chat),
> server-synced biomes, mob variants, paintings and trims, and the packets still ignored (camera, server
> links, message deletion).

### Settings that make sense on a phone

Every option you see works, and those a phone can't use don't clutter the menus.

> Done: Controller Off no longer turns the touch controls off; Entity Shadows, Chat Visibility and High
> Contrast do what they say; options Horizon stores but does not draw are greyed out; the controller
> page on a phone keeps what the touch controls use; Discord is gone on a phone. Left: a touch button
> for Zoom; the Keystrokes overlay following the touch stick; one graphics preset instead of vanilla's
> and Horizon's side by side.

## Next

- **A pack's own options in the game**: the settings screen a shader pack describes, beside its profile.
- **Secure chat**: signed chat and its acknowledgements, for servers that refuse unsigned messages.
- **Physical controllers on Android**: a gamepad backend for the Android build (the desktop one is off
  there).
- **The game files prepared on the phone**: today a computer with Java prepares them once from your own
  copy of the game.
- **Open to LAN and invites on a phone**: built in, not tried on Android yet.
- **The first release**: a signed APK on the releases page (every push already builds one in GitHub
  Actions).

## Later

- **A store release**, once the first releases have been played for a while.
- **More of the game open to mods**, guided by porting popular Java mods (see
  [mod-porting](../docs/mod-porting.md)).
- **A mod browser made for a phone.**
- **Discord Rich Presence on a phone**: Discord's Social SDK (1.10 and later) shows the game through the
  Discord app on Android without signing in, but it needs a Starling application on Discord's developer
  portal and the SDK downloaded from there; until then the setting stays hidden on Android.
