"""Prepare the repository-pinned native libraries needed by the full Cargo suite."""

import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import urllib.request

repo = Path(__file__).resolve().parents[2]
root = Path.home() / ".cache" / "codex-image-lab-voice"
archives = root / "archives"
native = root / "native"
archives.mkdir(parents=True, exist_ok=True)
manifest = json.loads((repo / "third_party/voice/sources.json").read_text())
if not (native / "built.json").is_file():
    for source in manifest["sources"]:
        archive = archives / source["archive"]
        if not archive.is_file():
            print(f"Downloading pinned {source['name']} {source['version']}", flush=True)
            urllib.request.urlretrieve(source["url"], archive)
        if hashlib.sha256(archive.read_bytes()).hexdigest() != source["sha256"]:
            raise RuntimeError(f"Archive checksum mismatch: {source['name']}")
    command = [
        sys.executable, str(repo / "third_party/voice/build_native.py"),
        "--archives", str(archives), "--output", str(native),
        "--target", "x86_64-unknown-linux-gnu", "--jobs", "2",
    ]
    for role, executable in {
        "cc": "gcc", "cxx": "g++", "cmake": "cmake", "make": "make",
        "pkg-config": "pkg-config", "shell": "bash", "ar": "ar", "ranlib": "ranlib",
    }.items():
        path = shutil.which(executable)
        if path is None:
            raise RuntimeError(f"Missing native build tool: {executable}")
        command.extend([f"--{role}", path])
    subprocess.run(command, check=True)
prefix = native / "prefix"
with Path(os.environ["GITHUB_ENV"]).open("a") as output:
    output.write(f"PKG_CONFIG_PATH={prefix}/lib/pkgconfig\n")
    output.write(f"LD_LIBRARY_PATH={prefix}/lib\n")
