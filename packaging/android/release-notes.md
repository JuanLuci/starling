**Starling 0.1.2: your name and your skin.**

## Install

- **On 0.1.0.1 or 0.1.1:** Starling offers this update on its title screen: tap **Update**, and Android installs it over (it asks you to allow installs from Starling the first time).
- **On 0.1.0, or new here:** download **`starling-0.1.2-arm64-v8a.apk`** below and open it. New players sign in with the Microsoft account that owns Minecraft: Java Edition, and the game is downloaded and prepared on the phone, once. You need a 64-bit phone with Android 8 or newer and Vulkan, and about 2 GB free.

> Installed a test build from GitHub Actions? Uninstall it first: test builds are signed with another key. Worlds kept in the `Starling` folder stay where they are.

## What's new

- **Your name and your skin**, in **Options > Skin Customization**: the name you play with in your worlds (3 to 16 letters, digits or `_`), and a skin from your files (**Choose a File...**, or a PNG in `Starling/skins`), classic or slim. Open a skin PNG with Starling and it is put on at once. Signed in with a Microsoft account, servers still see your account's name and skin.
- **The Starling folder, asked first**: the setup offers to keep your worlds, packs and logs in a `Starling` folder any file manager opens, before you sign in.
- **High refresh rates**: with Max Framerate on Unlimited, Starling asks Android for your screen's fastest refresh rate instead of letting it hold the game at 60 Hz.
- **Back from another app**: coming back to Starling from the browser after signing in, from the file picker or from any other app no longer leaves a black screen (it came with 0.1.1).

## Known issues

- Setting up with a real account has been tried up to the sign-in; preparing has been tried with the official files copied over. If the setup stops, **Try again** retries the step that failed, and the message now says why.
- Bedrock's buttons show from the start after the one that downloads them.
- Tested on a POCO F6 Pro (Adreno 740) and by a player on a vivo X200s (Immortalis-G925, a Mali GPU: about 100 fps); other Mali phones are untested so far.
- Servers that require signed chat (secure chat) refuse messages for now.
- Starling's own screens are in English only.

## Report a bug

Open an [issue](https://github.com/JuanLuci/starling/issues/new/choose): the form asks for your phone, your Android version and `Starling/logs/latest.log` (with `previous.log` beside it once Starling has been opened again).

---

Starling is unofficial: not an official Minecraft product, and not approved by or associated with Mojang or Microsoft. You need your own copy of Minecraft: Java Edition. The game is [Horizon](https://github.com/ludovic111/horizon), by Ludovic Marie; Starling, the Android port, is by [@JuanLuci](https://github.com/JuanLuci). MIT licensed.
