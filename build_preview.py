"""Build a harmless generic UI preview; never execute the resulting EXE."""

import argparse
import json
import os
import subprocess
from pathlib import Path

from build_support import (
    HERE,
    build_lock,
    build_plugin,
    find_makensis,
    verify_unelevated_manifest,
)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--renderer", choices=["raster", "svg"], default="svg")
    parser.add_argument("--maintenance", action="store_true")
    parser.add_argument(
        "--icon",
        type=Path,
        help="Optional consumer ICO; otherwise use the NSIS default",
    )
    parser.add_argument("--logo", type=Path, default=HERE / "ui/examples/logo.svg")
    parser.add_argument("--target-dir", type=Path)
    args = parser.parse_args()
    if os.name != "nt":
        parser.error("Windows and the MSVC toolchain are required")
    output = (
        HERE
        / "artifacts"
        / (
            "NativeNSIS-maintenance-preview.exe"
            if args.maintenance
            else f"NativeNSIS-preview-{args.renderer}.exe"
        )
    )
    output.parent.mkdir(parents=True, exist_ok=True)
    with build_lock(args.target_dir):
        dll = build_plugin(
            args.target_dir, features="native-svg" if args.renderer == "svg" else ""
        )
        defines = [f"-DOUTPUT={output}", f"-DPLUGIN_DIRECTORY={dll.parent}"]
        if args.renderer == "svg":
            defines.extend(["-DNATIVE_SVG", f"-DPREVIEW_LOGO={args.logo.resolve()}"])
        if args.maintenance:
            defines.append("-DMAINTENANCE_PREVIEW")
        if args.icon:
            defines.append(f"-DPREVIEW_ICON={args.icon.resolve()}")
        subprocess.run(
            [
                find_makensis(),
                "-INPUTCHARSET",
                "UTF8",
                "-WX",
                "-V3",
                *defines,
                str(HERE / "preview/setup.nsi"),
            ],
            check=True,
            timeout=120,
        )
        verify_unelevated_manifest(output)
    output.with_suffix(".build.json").write_text(
        json.dumps(
            {
                "renderer": args.renderer,
                "real_installer": False,
                "installer_bytes": output.stat().st_size,
                "ui_dll_bytes": dll.stat().st_size,
            },
            indent=2,
        ),
        encoding="utf-8",
    )
    print(f"Built {output} ({output.stat().st_size} bytes)")


if __name__ == "__main__":
    main()
