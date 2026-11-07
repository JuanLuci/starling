#!/usr/bin/env python3
"""Prepare Minecraft Java 1.21.11's official unobfuscated development jars for Horizon's port analysis.

The official release remains copyrighted game data: downloads stay in HORIZON_VERSIONS (outside Git),
and only metadata/class-index reports should be uploaded as CI artifacts.
"""
import argparse
import json
import shutil
import sys
import tempfile
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "tools"))

import class_index
import lab
import version_port as vp

VERSION = "1.21.11"
JAVA_MAJOR = 21
WORLD_VERSION = 4671
PROTOCOL_VERSION = 774
CLIENT_ZIP_URL = (
    "https://piston-data.mojang.com/v1/objects/"
    "e11114e8a2eea43bac93c022c9327d3916b24738/1_21_11_unobfuscated.zip"
)
CLIENT_ZIP_SHA1 = "e11114e8a2eea43bac93c022c9327d3916b24738"
SERVER_URL = (
    "https://piston-data.mojang.com/v1/objects/"
    "3ca78d5068bf9b422f694d3f0820e289581c0f0d/server.jar"
)
SERVER_SHA1 = "3ca78d5068bf9b422f694d3f0820e289581c0f0d"


def extract_client(archive_path, destination):
    """Find the official client jar inside Mojang's version ZIP and validate its version.json."""
    matches = []
    with zipfile.ZipFile(archive_path) as archive:
        members = [name for name in archive.namelist() if name.lower().endswith(".jar")]
        for member in members:
            with tempfile.NamedTemporaryFile(dir=destination.parent, suffix=".jar", delete=False) as stream:
                temporary = Path(stream.name)
                with archive.open(member) as source:
                    shutil.copyfileobj(source, stream)
            try:
                facts = vp.jar_facts(temporary)
                if (facts["world_version"] == WORLD_VERSION
                        and facts["protocol_version"] == PROTOCOL_VERSION
                        and facts["java_version"] == JAVA_MAJOR):
                    matches.append((member, temporary, facts))
                else:
                    temporary.unlink(missing_ok=True)
            except (KeyError, ValueError, zipfile.BadZipFile):
                temporary.unlink(missing_ok=True)
    if len(matches) != 1:
        for _, temporary, _ in matches:
            temporary.unlink(missing_ok=True)
        raise SystemExit(
            f"Expected exactly one 1.21.11 client jar in Mojang's ZIP; found {len(matches)}. "
            "Refusing to guess which jar to index."
        )
    member, temporary, facts = matches[0]
    destination.parent.mkdir(parents=True, exist_ok=True)
    temporary.replace(destination)
    print(f"Verified client jar from {member}: {facts}", flush=True)
    return facts


def prepare(args):
    store = vp.version_dir(VERSION)
    store.mkdir(parents=True, exist_ok=True)
    official = store / "official"
    official.mkdir(parents=True, exist_ok=True)
    client_zip = lab.download(CLIENT_ZIP_URL, official / "1_21_11_unobfuscated.zip", CLIENT_ZIP_SHA1)
    server = lab.download(SERVER_URL, store / "jars" / "server.jar", SERVER_SHA1)
    client = store / "jars" / "client.jar"
    facts = extract_client(client_zip, client)

    if facts["id"] not in {VERSION, VERSION + "_unobfuscated"}:
        raise SystemExit(f"Unexpected client identity: {facts['id']}")
    if facts["resource_pack"] != "75.0" or facts["data_pack"] != "94.1":
        raise SystemExit(f"Unexpected pack formats in client jar: {facts}")
    version_json = store / "version.json"
    with zipfile.ZipFile(client) as archive:
        version_json.write_bytes(archive.read("version.json"))

    server_game = vp.unpack_game(server, store / "jars" / "server-game.jar", VERSION)
    reports = store / "datagen" / "reports"
    if not reports.exists():
        print("Running Mojang's 1.21.11 data generator with Java 21", flush=True)
        vp.run_datagen(server, reports, JAVA_MAJOR)

    classes = class_index.index_jars([client, server_game], jobs=args.jobs)
    lab.write_json(store / "facts.json", facts)
    lab.write_json(store / "prepared.json", {
        "version": VERSION,
        "pinned": False,
        "source": "Mojang official 1.21.11 unobfuscated development artifacts",
        "paths": {
            "client": str(client.resolve()),
            "server": str(server.resolve()),
            "server_game": str(server_game.resolve()),
            "reports": str(reports.resolve()),
            "version_json": str(version_json.resolve()),
        },
    })
    lab.write_json(store / "classes.json", classes)
    print(f"Prepared {VERSION}: {len(classes)} indexed classes", flush=True)
    print(json.dumps(facts, indent=2))
    return 0


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("-j", "--jobs", type=int, default=None,
                        help="class indexing processes (default: CPU count)")
    return prepare(parser.parse_args())


if __name__ == "__main__":
    sys.exit(main())
