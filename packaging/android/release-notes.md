**Starling 0.1.0.1: updates from the game, a real keyboard, and Starling Visuals in the box.** Fixes and the things the first players asked for, a day after 0.1.0.

## Install

- **Coming from 0.1.0:** download **`starling-0.1.0.1-arm64-v8a.apk`** below and open it: it installs over 0.1.0 and keeps everything. This is the last time you do it by hand.
- **New here:** the same APK. Open Starling, sign in with the Microsoft account that owns Minecraft: Java Edition, and it downloads and prepares the game on the phone, once. You need a 64-bit phone with Android 8 or newer and Vulkan, and about 2 GB free.

> Installed a test build from GitHub Actions? Uninstall it first: test builds are signed with another key. Worlds kept in the `Starling` folder stay where they are.

## What's new

- **Updates from the game.** When a newer release is out, the title screen says so: **Update** downloads it and Android installs it over this one (it asks you to allow installs from Starling the first time), **Tomorrow** asks again in a day, **No** at the next start.
- **The phone's own keyboard.** Suggestions, autocorrect, swipe typing and paste now work in the chat, signs and books. Commands are left alone by autocorrect, and **Enter finishes a command** when only one completion fits: `/gamemode c` runs `/gamemode creative`.
- **Starling Visuals comes with the app.** Pick it in **Video Settings > Shader Packs**: no zip to copy. The page also says where its folder is.
- **Every language.** The game is prepared on the phone with all the languages Minecraft has, in **Options > Language**.
- **The Starling folder, again.** Said "Not now" to keeping your files in a `Starling` folder? The **Open Pack Folder** buttons offer it again.

## Fixes

- **Touch Controls** (Options > Controls) opens the layout editor while Starling sets up and on the title screen, not only in a world: lay out your controls while the game prepares.
- The credits (Options > Credits & Attribution) close with a tap or the back key; while setting up they could not be closed at all.
- No "No such file or directory" under the setup card on a first start.

## Known issues

- Setting up with a real account has been tried up to the sign-in; preparing has been tried with the official files copied over. If the setup stops, **Try again** retries the step that failed.
- Keep Starling open while it prepares the game: leaving it can stop the preparing, and it starts over.
- Tested on one phone (POCO F6 Pro, Adreno 740); phones with Mali GPUs are untested so far.
- Servers that require signed chat (secure chat) refuse messages for now.
- Starling's own screens are in English only.

## Report a bug

Open an [issue](https://github.com/JuanLuci/starling/issues/new/choose): the form asks for your phone, your Android version and `Starling/logs/latest.log`.

---

Starling is unofficial: not an official Minecraft product, and not approved by or associated with Mojang or Microsoft. You need your own copy of Minecraft: Java Edition. The game is [Horizon](https://github.com/ludovic111/horizon), by Ludovic Marie; Starling, the Android port, is by [@JuanLuci](https://github.com/JuanLuci). MIT licensed.
