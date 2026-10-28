**Starling 0.1.1: Bedrock's buttons, a world that keeps, and Horizon brought up to date.**

## Install

- **On 0.1.0.1:** Starling offers this update on its title screen: tap **Update**, and Android installs it over (it asks you to allow installs from Starling the first time).
- **On 0.1.0, or new here:** download **`starling-0.1.1-arm64-v8a.apk`** below and open it. New players sign in with the Microsoft account that owns Minecraft: Java Edition, and the game is downloaded and prepared on the phone, once. You need a 64-bit phone with Android 8 or newer and Vulkan, and about 2 GB free.

> Installed a test build from GitHub Actions? Uninstall it first: test builds are signed with another key. Worlds kept in the `Starling` folder stay where they are.

## What's new

- **Bedrock's touch buttons.** The controls wear Bedrock's own buttons, which Starling downloads from Mojang after preparing the game (nothing of Mojang's is in the app). Offline, Starling's own stone-grey buttons stand in.
- **Sneak as Hold or Toggle**, picked in the layout editor. Flying, Sneak only takes you down while you hold it.
- **Lay out your controls anywhere**, while the game prepares and on the title screen too.
- **Your world keeps** when you close Starling abruptly: it saves as soon as it leaves the screen.
- **Fewer anticheat flags.** Breaking a block while moving no longer reaches the server late (GrimAC flagged players digging up while flying), and a break the server refuses puts you back where you stood, as Java does.
- **Horizon brought up to date**: 2118 of its commits, with many fixes to the game.

## Fixes

- When the setup cannot reach Microsoft, Mojang or GitHub, it now says why (the network's own words), for a bug report.
- A world whose save was cut short opens from its backup `level.dat_old`.
- After an abrupt close the last session's log is kept as `Starling/logs/previous.log`.

## Known issues

- Setting up with a real account has been tried up to the sign-in; preparing has been tried with the official files copied over. If the setup stops, **Try again** retries the step that failed.
- Bedrock's buttons show from the start after the one that downloads them.
- Tested on one phone (POCO F6 Pro, Adreno 740); phones with Mali GPUs are untested so far.
- Servers that require signed chat (secure chat) refuse messages for now.
- Starling's own screens are in English only.

## Report a bug

Open an [issue](https://github.com/JuanLuci/starling/issues/new/choose): the form asks for your phone, your Android version and `Starling/logs/latest.log` (with `previous.log` beside it once Starling has been opened again).

---

Starling is unofficial: not an official Minecraft product, and not approved by or associated with Mojang or Microsoft. You need your own copy of Minecraft: Java Edition. The game is [Horizon](https://github.com/ludovic111/horizon), by Ludovic Marie; Starling, the Android port, is by [@JuanLuci](https://github.com/JuanLuci). MIT licensed.
