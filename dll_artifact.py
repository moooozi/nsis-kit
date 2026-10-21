"""Build and verify the production DLL artifact for the pinned NSIS toolkit."""

import argparse
import hashlib
import json
import shutil
import subprocess
import sys
from pathlib import Path

TOOLKIT = Path(__file__).resolve().parent
DLL_NAME = "native_nsis_ui.dll"
TARGET = "i686-pc-windows-msvc"


def toolkit_revision():
    return subprocess.check_output(
        ["git", "-C", str(TOOLKIT), "rev-parse", "HEAD"], text=True
    ).strip()


def metadata(dll, revision):
    return {
        "schema": 1,
        "toolkit_revision": revision,
        "target": TARGET,
        "features": ["installer"],
        "sha256": hashlib.sha256(dll.read_bytes()).hexdigest(),
    }


def verify_artifact(directory, revision=None):
    dll = directory / DLL_NAME
    actual = json.loads((directory / "plugin.json").read_text(encoding="utf-8"))
    expected = metadata(dll, revision if revision is not None else toolkit_revision())
    if not dll.stat().st_size or actual != expected:
        raise ValueError(
            "NSIS DLL artifact does not match the pinned production toolkit"
        )
    return dll.resolve()


def build_artifact(directory):
    sys.path.insert(0, str(TOOLKIT))
    from build_support import build_lock, build_plugin

    with build_lock():
        # This also loads the DLL in the x86 export checker and rejects fixture code.
        dll = build_plugin(features="installer")
        directory.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(dll, directory / DLL_NAME)
        (directory / "plugin.json").write_text(
            json.dumps(metadata(directory / DLL_NAME, toolkit_revision()), indent=2)
            + "\n",
            encoding="utf-8",
        )
    verify_artifact(directory)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path)
    parser.add_argument("--verify", action="store_true")
    args = parser.parse_args()
    if args.verify:
        verify_artifact(args.directory)
    else:
        build_artifact(args.directory)


if __name__ == "__main__":
    main()
