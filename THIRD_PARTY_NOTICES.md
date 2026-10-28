# Third-party notices

## wgpu-hal

`third_party/wgpu-hal` is [wgpu-hal](https://github.com/gfx-rs/wgpu) 30.0.1 by the gfx-rs
developers, with one upstream fix applied
([gfx-rs/wgpu#10124](https://github.com/gfx-rs/wgpu/pull/10124)). It is licensed under MIT or
Apache-2.0; both licenses are in that folder (`LICENSE.MIT`, `LICENSE.APACHE`).

## Pixelify Sans

`crates/horizon-theme/fonts/PixelifySans-*.ttf` is [Pixelify Sans](https://github.com/eifetx/Pixelify-Sans)
by The Pixelify Sans Project Authors, licensed under the SIL Open Font License 1.1
(`crates/horizon-theme/fonts/OFL-PixelifySans.txt`).

## Pixelarticons

`crates/horizon-theme/icons/pixel/*.svg` are icons from
[Pixelarticons](https://github.com/halfmage/pixelarticons) by Gerrit Halfmann, under the MIT
license (`crates/horizon-theme/icons/pixel/LICENSE-pixelarticons.txt`). `star.svg` is Horizon's own, drawn on
the same grid.

## OpenJDK for Android (fetched on the phone, not shipped)

To prepare the game files on a phone, Starling downloads OpenJDK 25.0.5 built for Android by
AngelAuraMC ([angelauramc-openjdk-build](https://github.com/AngelAuraMC/angelauramc-openjdk-build),
release `download_jre25`, checked against the SHA-256 in `crates/starling-account/src/java.rs`),
runs it for the preparing and deletes it after. OpenJDK is licensed under the GNU General Public
License version 2 with the Classpath Exception; its sources are at
[openjdk/jdk](https://github.com/openjdk/jdk), the build's scripts and patches in that
repository. Starling's launcher for it (`packaging/android/native/java_launcher.c`) is MIT.
