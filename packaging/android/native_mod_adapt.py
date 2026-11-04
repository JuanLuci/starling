#!/usr/bin/env python3
"""Native mods made before Horizon split its client into crates, pointed at where things went.

A mod names what it hooks by crate and module, and is built by Horizon at the commit it pins. Starling
builds every mod at its own commit (native_build.sh), and at that commit `horizon_client` holds only the
game module and a glob of `horizon_client_lib`, which horizon-mixins can't look through. Only the
manifests change: what a mod's code does with what moved is the compiler's to judge.

    native_mod_adapt.py SOURCE MODS    SOURCE is a checkout, MODS holds <bundle>/horizon-mixin.toml
"""
import pathlib
import re
import sys

ROOTS = {
    "horizon_client_lib": "crates/horizon-client/src/client_lib.rs",
    "horizon_client_render": "crates/horizon-client-render/src/lib.rs",
    "horizon_client_core": "crates/horizon-client-core/src/lib.rs",
    "horizon_client_input": "crates/horizon-client-input/src/lib.rs",
}
FOLDERS = ("crates/horizon-client-render/src/", "crates/horizon-client-core/src/", "crates/horizon-client-input/src/")
OLD_FOLDER = "crates/horizon-client/src/"
OLD_ROOT = OLD_FOLDER + "lib.rs"
# What Horizon renamed since, and takes the same arguments.
RENAMED = {"horizon_client::game::titles::Game<'_>::draw_titles": "horizon_client::game::titles::Game<'_>::draw_title"}
MODULE = re.compile(r"^\s*(?:pub(?:\(crate\))? )?mod (\w+)\s*(?:;|\{)", re.M)


def owners(source):
    """Each top-level module's crate, the first root that declares it."""
    found = {}
    for crate, root in ROOTS.items():
        for module in MODULE.findall((source / root).read_text(encoding="utf-8")):
            found.setdefault(module, crate)
    return found


def adapt(text, source, modules):
    def target(match):
        path = match[2]
        for old, new in RENAMED.items():
            path = path.replace(old, new)
        path = re.sub(
            r"\bhorizon_client::(\w+)",
            lambda m: f"{modules[m[1]]}::{m[1]}" if m[1] in modules else m[0],
            path,
        )
        return f'{match[1]}{path}"'

    def file(match):
        path = match[2]
        if path == OLD_ROOT:
            path = ROOTS["horizon_client_lib"]
        elif path.startswith(OLD_FOLDER) and not (source / path).exists():
            moved = (folder + path[len(OLD_FOLDER) :] for folder in FOLDERS)
            path = next((p for p in moved if (source / p).exists()), path)
        return f'{match[1]}{path}"'

    text = re.sub(r'^(\s*target\s*=\s*")([^"]*)"', target, text, flags=re.M)
    return re.sub(r'^(\s*file\s*=\s*")([^"]*)"', file, text, flags=re.M)


def main():
    source, mods = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2])
    modules = owners(source)
    for manifest in mods.glob("*/horizon-mixin.toml"):
        old = manifest.read_text(encoding="utf-8")
        new = adapt(old, source, modules)
        if new != old:
            manifest.write_text(new, encoding="utf-8")
            print(f"native_mod_adapt: {manifest.parent.name} pointed at where things went")


if __name__ == "__main__":
    main()
