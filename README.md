<p align="center">
  <img src="https://img.shields.io/badge/Minecraft-Java%2026.3-5D9C3F" alt="Minecraft Java 26.3">
  <img src="https://img.shields.io/badge/Rust-stable-B7410E?logo=rust&logoColor=white" alt="Rust">
  <a href="https://github.com/ludovic111/horizon"><img src="https://img.shields.io/badge/fork%20of-Horizon-F0795A" alt="Fork of Horizon"></a>
  <img src="https://img.shields.io/badge/platform-Android-2A2470" alt="Android">
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-3B2F7E" alt="MIT license"></a>
</p>

<p align="center">
  <a href="#download"><b>Download</b></a> ·
  <a href="#android-status"><b>Android status</b></a> ·
  <a href="#build-from-source"><b>Build from source</b></a> ·
  <a href="CONTRIBUTING.md"><b>Contributing</b></a> ·
  <a href="https://github.com/ludovic111/horizon"><b>Horizon</b></a>
</p>

# Starling

**Starling is a lightweight port of Minecraft: Java Edition 26.3 to Android.** It is a fork of [Horizon](https://github.com/ludovic111/horizon), the Java game rewritten in Rust: it reads the same worlds, follows the same rules and generates the same terrain as Java Edition, and is checked against it by 281 differential harnesses. You play it on the files from your own copy of Minecraft; it is free, open source and unofficial. And it runs natively, with no Java virtual machine and no OpenGL translation in between, so it starts fast and plays smoothly on a phone.

> Not an official Minecraft product. Not approved by or associated with Mojang or Microsoft. Starling contains no Minecraft code, assets or data: you need your own legally obtained copy of Minecraft: Java Edition 26.3.

## Download

There is no release yet: for now, [build the APK from source](#build-from-source). Releases will be published on [the releases page](https://github.com/JuanLuci/starling/releases).

**You need** a 64-bit phone with Android 8 or newer and Vulkan; Minecraft: Java Edition 26.3; and, for now, a computer with a Java 25 JDK to prepare the game files once from your own copy. Zip the folder it makes, name it `.starling` and open it with Starling on the phone.

## What works today

- **The game:** singleplayer with vanilla's world generation, mobs, villagers, raids, redstone, every command, the HUD and the recipe book, on saves in vanilla's format. Worlds from 1.8 to 26.2 are upgraded as vanilla does.
- **Touch controls:** every action on screen, from the movement stick to the hotbar, chat and commands, with a layout you can move, resize and fade in the game.
- **Your files within reach:** worlds, resource packs, shader packs, mods and screenshots live in a `Starling` folder on the phone's storage, which any file manager opens. Open a `.starling` file to add a pack, a world or a mod.
- **Made for phones:** settings picked for your device on the first start, and a stats overlay (tap with three fingers) for FPS, memory and temperature.
- **Horizon's own extras:** new menus (vanilla's are one setting away), graphics presets and more, all off by default where they would change the vanilla look. See [Settings](https://playhorizon.dev/docs/settings).
- **Resource packs and data packs:** resource packs work, and data packs load functions, predicates and item modifiers.

## Android status

What the Android port covers so far. The game itself is Horizon's: its parity audit, measured on the desktop, is in [Horizon's project status](https://github.com/ludovic111/horizon#project-status) ([as of Starling's base](docs/horizon-parity.md)).

| Part | Progress | Notes |
| --- | --- | --- |
| The game, singleplayer | `██████████` | Horizon's game, as on the desktop |
| Touch controls | `██████████` | Every action, with an editable layout |
| Starling folder and `.starling` files | `█████████░` | Packs and game files checked on a phone; worlds and mods by tests only |
| Resource packs | `█████████░` | Pack icons show the unknown-pack texture, as in Horizon |
| Performance tuning | `████████░░` | Tuned on one phone; mid-range devices not measured yet |
| Shader packs | `██░░░░░░░░` | Built in, not tried on Android yet |
| Mods | `██░░░░░░░░` | Horizon's mod system is built in, not tried on Android yet |
| Playing with others | `█░░░░░░░░░` | Invites, Open to LAN and servers not tried on Android yet |
| Physical controllers | `░░░░░░░░░░` | No Android backend yet |
| Preparing the game files on the phone | `░░░░░░░░░░` | Needs a computer today |
| Microsoft account sign-in | `░░░░░░░░░░` | Not yet: online-mode servers can't be joined |

## Build from source

For developers. Requirements: Rust 1.90 or newer with the `aarch64-linux-android` target, [cargo-ndk](https://github.com/bbqsrc/cargo-ndk), the Android SDK (build-tools 35.0.0, platform 36, NDK r28c), a JDK 25 and Python 3.10 or newer.

```sh
git clone https://github.com/JuanLuci/starling.git
cd starling
python packaging/android/build_apk.py            # the APK lands in target/android/
```

Starling contains no game files and never downloads them. It builds its resources from **your own copy** of Minecraft: Java Edition 26.3, verified against the checksums in `research-lock.json`, the same way Horizon does:

1. **Client:** install Java Edition with the official launcher and play 26.3 once. Horizon reads `versions/26.3/`, `libraries/` and `assets/` from the launcher's game directory. If yours lives elsewhere, pass `--minecraft-dir` or set `HORIZON_MINECRAFT_DIR`.
2. **Server jar:** the 26.3 server jar, from the `downloads.server.url` in your launcher's `versions/26.3/26.3.json`. Pass it with `--server-jar` or set `HORIZON_SERVER_JAR`.

```sh
cargo run --release -p horizon-prepare -- --prepare --server-jar ~/Downloads/server.jar   # needs JDK 25
```

The prepared files land in the gitignored `.local/horizon-client/26.3`. Zip that folder, rename the zip to `.starling` and open it with Starling. It is derived from the game: never commit it or share it.

## Contributing

Issues and pull requests are welcome: see [CONTRIBUTING.md](CONTRIBUTING.md). Changes to the game itself belong upstream, in [Horizon](https://github.com/ludovic111/horizon); Starling is the Android app around it.

## Licence

[MIT](LICENSE), for the project's original work only, as Horizon's. Dependencies keep their own licences. Minecraft is a trademark of Mojang Synergies AB; Minecraft code and assets belong to Mojang and Microsoft and are covered by the [Minecraft EULA](https://www.minecraft.net/en-us/eula). Prepared resources stay on your devices and are never published.

Third-party credits and licenses: [Third-party notices](THIRD_PARTY_NOTICES.md).

## Author

Starling, the Android port, by JuanLuci ([@JuanLuci](https://github.com/JuanLuci)). Horizon made by [Ludovic Marie](https://lu4ovic.com) ([@ludovic111](https://github.com/ludovic111)).

Tested on a POCO F6 Pro (Snapdragon 8 Gen 2, Adreno 740, 12 GB of RAM, Android 16).
