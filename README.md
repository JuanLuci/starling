<p align="center">
  <img src="https://img.shields.io/badge/Minecraft-Java%2026.3-5D9C3F" alt="Minecraft Java 26.3">
  <img src="https://img.shields.io/badge/Rust-stable-B7410E?logo=rust&logoColor=white" alt="Rust">
  <a href="https://github.com/ludovic111/horizon"><img src="https://img.shields.io/badge/fork%20of-Horizon-F0795A" alt="Fork of Horizon"></a>
  <img src="https://img.shields.io/badge/platform-Android-2A2470" alt="Android">
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-3B2F7E" alt="MIT license"></a>
</p>

<p align="center">
  <a href="#download"><b>Download</b></a> ·
  <a href="#android-status"><b>Android Status</b></a> ·
  <a href="roadmap/starling.md"><b>Roadmap</b></a> ·
  <a href="#mods"><b>Mods</b></a> ·
  <a href="#build-from-source"><b>Build from Source</b></a> ·
  <a href="CONTRIBUTING.md"><b>Contributing</b></a> ·
  <a href="https://github.com/ludovic111/horizon"><b>Horizon</b></a>
</p>

<h1 align="center">Starling</h1>

<p align="center">
  <b>A lightweight port of Minecraft: Java Edition 26.3 to Android</b>
</p>

> **Not an official Minecraft product.** Not approved by or associated with Mojang or Microsoft. Starling contains no Minecraft code, assets, or data: you need your own legally obtained copy of Minecraft: Java Edition 26.3.

---

## ✨ Overview

**Starling** is a fork of [Horizon](https://github.com/ludovic111/horizon) — Minecraft: Java Edition rewritten in Rust. It reads the same worlds, follows the same rules, and generates the same terrain as Java Edition, verified against it by differential harnesses.

You play using files from your own copy of Minecraft. It's free, open source, and unofficial. Running natively with no Java Virtual Machine and no OpenGL translation layer, it starts fast and plays smoothly on a phone.

---

## 📥 Download

| Version | Link |
|---------|------|
| **Starling v0.2.1** | [Latest Release](https://github.com/JuanLuci/starling/releases/latest) |

**Already have Starling?** Tap **Update** inside the game.

**New here?** Download `starling-0.2.1-arm64-v8a.apk` from [the releases page](https://github.com/JuanLuci/starling/releases) and open it on your phone (Android will ask to allow installing apps from your browser or file manager).

### Requirements

- 64-bit phone with Android 8+ and Vulkan support
- ~2 GB free storage
- Microsoft account that owns Minecraft: Java Edition (or play offline — most servers require sign-in)

### First Launch

On first start, Starling downloads the game's files from Mojang's servers and prepares them on the phone (takes ~79 seconds on a POCO F6 Pro, longer on slower devices). You can go fishing or change settings while it works.

### Nightly Builds

Builds from every push to the `android` branch are available in [GitHub Actions](https://github.com/JuanLuci/starling/actions/workflows/android.yml) (the `starling-apk` artifact; requires GitHub sign-in to download). These are signed with a different key than releases — uninstall one before installing the other, after copying your worlds out if they're not in the `Starling` folder.

You can also [build from source](#build-from-source).

---

## ✅ What Works Today

| Feature | Status |
|---------|--------|
| **Setup on Phone** | Sign in with Microsoft account; game files downloaded from Mojang and prepared on-device using a temporary Java runtime (removed after). All game languages supported. While preparing, fish in a pond or tweak settings in a hand-drawn skin. |
| **Core Gameplay** | Singleplayer with vanilla world generation, mobs, villagers, raids, redstone, every command, HUD, recipe book. Worlds in vanilla format, saved when app backgrounds. Worlds from 1.8→26.2 upgraded as vanilla does. Tracks Horizon closely (v0.2.0 based on Horizon main from 2026-11-02). |
| **Touch Controls** | Every action on screen: movement stick, hotbar, chat, commands, F3. Editable layout (move, resize, fade). Sneak as Hold/Toggle. Bedrock control modes: stick (floating/fixed) or D-pad, tap-to-interact. F3 combos (chunk borders, hitboxes, tooltips). Pick Block & player list as toggle buttons. Haptic feedback on block break/stack split. Gyroscope aim (off by default). Uses Bedrock's buttons fetched from Mojang's samples repo after prep. |
| **Custom Buttons** | Add via **Touch Controls → Add** or **Options → Controls → Custom Buttons**. Each holds/toggles a key, native mod key, sends a command or chat line. Custom icons or your own images (PNG), drawn on Bedrock's plate. |
| **Bedrock Inventory** | Creative inventory shows items + your inventory side-by-side, 6 rows + 2×2 crafting grid, tap for full stack. Enabled by default; disable in **Options → Controls** for Java layout. |
| **Name & Skin** | **Options → Skin Customization**: name for worlds, skin from files (or `Starling/skins`), classic/slim. Open a skin PNG with Starling to apply instantly. |
| **System Keyboard** | Tap text field → keyboard appears. Tap beside → dismiss. Suggestions, autocorrect, swipe typing, clipboard in chat/signs/books. Commands/searches skip autocorrect. Paste button on chat bar. Enter runs command when only one completion fits (`/gamemode c` → `/gamemode creative`). |
| **File Access** | Worlds, resource packs, shader packs, mods, screenshots in `Starling` folder on device storage. Any file manager can open it (re-offered on first start if declined, also via Open Pack Folder buttons). Open `.starling`, `.hmod`, `.hnmod` files with Starling to add packs/worlds/mods. |
| **Phone Optimizations** | Settings auto-picked for your device on first start. Stats overlay for FPS, memory, temperature. |
| **Horizon Extras** | New menus (vanilla's one setting away), graphics presets, more — all off by default where they'd change vanilla look. See [Settings](https://playhorizon.dev/docs/settings). |
| **Resource & Data Packs** | Resource packs work. Data packs load functions, predicates, item modifiers. |
| **Servers** | Offline-mode servers, behind proxies (Velocity), with resource packs, colored chat, boss bars, signs, workstation menus, flight, attributes. Plugin server dialogs, chat buttons, custom armour (ItemsAdder, Nexo), models riding mobs (ModelEngine, BetterModel), clickable furniture. Touch stick movement modes per server (Free Direction, D-pad, Analog) remembered per server, accepted by anticheats (GrimAC, etc.). |
| **Shader Packs** | Iris/OptiFine packs (Complementary, MakeUp, BSL, Photon, Bliss) run on phone, translated on load. Place in `Starling/shaderpacks`, pick in **Video Settings → Shader Packs**. **Auto** profile fits to phone, plus **Shader Options**, **Resolution**, **FPS Limit**. On Snapdragon 8 Gen 2 (Auto): MakeUp 87 fps, Complementary Reimagined 59, BSL 54, Complementary Unbound 39, Photon 27–39. See [roadmap](roadmap/starling.md). |
| **Starling Visuals** | Built-in shader pack for phones. Pick in **Video Settings → Shader Packs** ([packaging/shaderpacks/starling-visuals](packaging/shaderpacks/starling-visuals), also `StarlingVisuals.zip` on releases). Sunlight/shadows, waved water with depth, haze, sun rays through trees/clouds, bloom, handheld torch light. |
| **Mods** | Horizon's native mods (`.hnmod`, compiled on-device) and content mods (`.hmod`). See [Mods](#mods). |

---

## 📊 Android Status

Progress of the Android port. Current work and upcoming items are in the [roadmap](roadmap/starling.md). The game itself is Horizon's — its parity audit (measured on desktop) is in [Horizon's project status](https://github.com/ludovic111/horizon#project-status) ([as of Starling's base](docs/horizon-parity.md)).

| Part | Progress | Notes |
|------|----------|-------|
| The game, singleplayer | ![100%](docs/media/progress/100.svg) | Horizon's game, as on desktop |
| Touch controls | ![100%](docs/media/progress/100.svg) | Every action, editable layout & custom buttons; stick/D-pad/Free Direction on servers; Bedrock buttons from Mojang |
| Starling folder, `.starling`, `.hmod`, `.hnmod` files | ![100%](docs/media/progress/100.svg) | Packs, worlds, mods, game files opened with Starling |
| Resource packs | ![100%](docs/media/progress/100.svg) | Server packs too, loaded without freezing |
| Performance tuning | ![95%](docs/media/progress/95.svg) | Settings per phone, resolution down to 25% for weaker devices; mid-range not yet measured |
| Settings | ![100%](docs/media/progress/100.svg) | Unusable options greyed out, unavailable hidden |
| Mods | ![95%](docs/media/progress/95.svg) | Native mods compiled on-device, tested end-to-end; content mods; registry mods pass pending |
| Servers | ![100%](docs/media/progress/100.svg) | Offline-mode & proxies, dialogs, chat buttons/links, secure chat with signed-in account |
| Server plugins | ![100%](docs/media/progress/100.svg) | Custom armour from any pack, animated models on mobs, block/item displays, clickable furniture, item cooldowns, HUD text without shadows |
| Movement on anticheat servers | ![95%](docs/media/progress/95.svg) | Free Direction/D-pad/Analog per server, no pull-backs on GrimAC/Pika in tests; vanilla-predicted block breaking; some old-version servers behind ViaVersion still pull back |
| Shader packs | ![95%](docs/media/progress/95.svg) | Complementary, MakeUp, BSL, Photon, Bliss run on phone with Auto profile & Shader Options; some effects still differ |
| APK builds & updates | ![100%](docs/media/progress/100.svg) | Signed releases on releases page, offered & installed by game from v0.1.0.1 (first offered: v0.1.1); every push builds APK in Actions |
| Open to LAN & invites | ![95%](docs/media/progress/95.svg) | Horizon's, in-game; little phone testing so far |
| Physical controllers | ![100%](docs/media/progress/100.svg) | Android gamepads, tested on phone |
| Setup on phone | ![100%](docs/media/progress/100.svg) | Prepared in 79 s from official 26.3 files, Java fetched for it; fishing & hand-drawn settings while working |
| Microsoft account sign-in | ![100%](docs/media/progress/100.svg) | On first start and in-game (title screen corner); code & ownership check work on phone |

---

## 🧩 Mods

Starling runs Horizon's mods. Open the file with Starling (from file manager or downloads) and it appears in **Options → Mods**.

### `.hnmod` — Native Mods
Tap **Compile**. Starling warns of risks, builds the game with your mods while you play, and restarts with them loaded. Takes a few minutes, needs internet. Recompile after updates. Native mods run as part of the game — install only what you trust. See [native mods](docs/native-mods.md) and [how Starling builds them](docs/starling-native-mods.md); [`wrench`](native-mods/wrench) is an example.

### `.hmod` — Content Mods
Data-only mods, nothing to compile. See [mods](docs/mods.md).

> The old Starling mods and the WebAssembly mod format of v0.1 are gone.

---

## 🛠 Build from Source

For developers.

### Requirements
- Rust 1.90+ with `aarch64-linux-android` target
- [cargo-ndk](https://github.com/bbqsrc/cargo-ndk)
- Android SDK: build-tools 35.0.0, platform 36, NDK r28c
- JDK 25
- Python 3.10+

### Build
```sh
git clone https://github.com/JuanLuci/starling.git
cd starling
python packaging/android/build_apk.py            # APK lands in target/android/
```

The setup's hand-drawn skin is generated by `python packaging/lowbudget/make.py` (requires Pillow) into `packaging/lowbudget/pack`, bundled into the APK as-is.

### Preparing Game Files (Optional)
The APK contains no game files. On the phone, Starling downloads and prepares them from Mojang. To prepare on a computer instead (from **your own copy** of Minecraft: Java Edition 26.3, verified against `research-lock.json` checksums, same as Horizon):

1. **Client:** Install Java Edition via official launcher, play 26.3 once. Horizon reads `versions/26.3/`, `libraries/`, `assets/` from the launcher's game directory. Override with `--minecraft-dir` or `HORIZON_MINECRAFT_DIR` if needed.
2. **Server JAR:** From `downloads.server.url` in your launcher's `versions/26.3/26.3.json`. Pass with `--server-jar` or `HORIZON_SERVER_JAR`.

```sh
cargo run --release -p horizon-prepare -- --prepare --server-jar ~/Downloads/server.jar   # needs JDK 25
```

Prepared files go to `.local/horizon-client/26.3`. Zip that folder, rename to `.starling`, open with Starling. **Derived from the game — never commit or share it.**

---

## 🤝 Contributing

Issues and pull requests welcome — see [CONTRIBUTING.md](CONTRIBUTING.md). Changes to the game itself belong upstream in [Horizon](https://github.com/ludovic111/horizon); Starling is the Android app around it.

---

## 📄 Licence

[MIT](LICENSE) — for the project's original work only, as Horizon's. Dependencies keep their own licences.

Minecraft is a trademark of Mojang Synergies AB; Minecraft code and assets belong to Mojang and Microsoft, covered by the [Minecraft EULA](https://www.minecraft.net/en-us/eula). Prepared resources stay on your devices and are never published.

Third-party credits and licenses: [Third-party notices](THIRD_PARTY_NOTICES.md).

---

## 👤 Author

**Starling** (Android port) by **JuanLuci** ([@JuanLuci](https://github.com/JuanLuci)).

**Horizon** by **Ludovic Marie** ([lu4ovic.com](https://lu4ovic.com), [@ludovic111](https://github.com/ludovic111)).

---

## 📱 Tested Devices

- POCO F6 Pro (Snapdragon 8 Gen 2, Adreno 740, 12 GB RAM, Android 16)
- Tecno Pova 7
- Mali-G57, Mali-G68 GPUs
- Redmi Note 8 (Mali-G52)
- Many others