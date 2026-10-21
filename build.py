"""Build a configured Windows x64 installer. Never run the resulting EXE."""

import argparse
import os
import subprocess
from pathlib import Path

from build_support import (
    build_lock,
    build_plugin,
    find_makensis,
    verify_payload_architecture,
    verify_unelevated_manifest,
)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--script", required=True, type=Path, help="Consumer NSIS entry point"
    )
    parser.add_argument(
        "--payload", required=True, type=Path, help="Prebuilt Windows x64 application"
    )
    parser.add_argument(
        "--version", required=True, help="Three-component numeric Windows version"
    )
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument(
        "--target-dir", type=Path, help="Private Cargo output directory"
    )
    args = parser.parse_args()
    if os.name != "nt":
        parser.error("Windows and the MSVC toolchain are required")
    parts = args.version.split(".")
    if len(parts) != 3 or not all(
        part.isascii() and part.isdecimal() and int(part) <= 65535 for part in parts
    ):
        parser.error("--version must be three numeric components, each 0..65535")
    payload, output, script = (
        p.resolve() for p in [args.payload, args.output, args.script]
    )
    verify_payload_architecture(payload)
    output.parent.mkdir(parents=True, exist_ok=True)
    with build_lock(args.target_dir):
        dll = build_plugin(args.target_dir)
        subprocess.run(
            [
                find_makensis(),
                "-INPUTCHARSET",
                "UTF8",
                "-WX",
                "-V3",
                f"-DVERSION={args.version}",
                f"-DPAYLOAD_EXE={payload}",
                f"-DOUTPUT={output}",
                f"-DPLUGIN_DIRECTORY={dll.parent}",
                str(script),
            ],
            check=True,
            timeout=120,
        )
        verify_unelevated_manifest(output)
    print(f"Built {output} ({output.stat().st_size} bytes)")


if __name__ == "__main__":
    main()
