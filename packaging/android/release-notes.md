**Starling 0.1.3: signing in works when your phone cuts background apps off the internet.**

## Install

- **On 0.1.0.1 or later:** Starling offers this update on its title screen: tap **Update**, and Android installs it over (it asks you to allow installs from Starling the first time).
- **On 0.1.0, or new here:** download **`starling-0.1.3-arm64-v8a.apk`** below and open it. New players sign in with the Microsoft account that owns Minecraft: Java Edition, and the game is downloaded and prepared on the phone, once. You need a 64-bit phone with Android 8 or newer and Vulkan, and about 2 GB free.

> Installed a test build from GitHub Actions? Uninstall it first: test builds are signed with another key. Worlds kept in the `Starling` folder stay where they are.

## What's fixed

- **Signing in no longer stops with "Could not reach Microsoft (dns failed ...)"** after you entered the code in the browser. While you sign in there, Starling waits in the background, and many phones cut a background app's internet (battery and data savers, the maker's own restrictions): the first answer it missed ended the sign-in. Now it keeps asking until you come back, and the steps after it (Xbox, Minecraft) are tried again too.

## Known issues

- Setting up with a real account has been tried up to the sign-in; preparing has been tried with the official files copied over. If the setup stops, **Try again** retries the step that failed, and the message says why.
- Bedrock's buttons show from the start after the one that downloads them.
- Tested on a POCO F6 Pro (Adreno 740) and by a player on a vivo X200s (Immortalis-G925, a Mali GPU: about 100 fps); other Mali phones are untested so far.
- Servers that require signed chat (secure chat) refuse messages for now.
- Starling's own screens are in English only.

## Report a bug

Open an [issue](https://github.com/JuanLuci/starling/issues/new/choose): the form asks for your phone, your Android version and `Starling/logs/latest.log` (with `previous.log` beside it once Starling has been opened again).

---

Starling is unofficial: not an official Minecraft product, and not approved by or associated with Mojang or Microsoft. You need your own copy of Minecraft: Java Edition. The game is [Horizon](https://github.com/ludovic111/horizon), by Ludovic Marie; Starling, the Android port, is by [@JuanLuci](https://github.com/JuanLuci). MIT licensed.
