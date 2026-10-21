"""Run the standalone toolkit checks from any working directory (Windows only)."""

import argparse
import os
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
MANIFEST = "ui/Cargo.toml"
TARGET = "i686-pc-windows-msvc"


def run(*command):
    print(f"> {subprocess.list2cmdline(command)}", flush=True)
    subprocess.run(command, cwd=HERE, check=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--skip-toolchain-setup",
        action="store_true",
        help="Use already installed Rust components and targets.",
    )
    args = parser.parse_args()
    if os.name != "nt":
        parser.error("These checks require Windows and the MSVC toolchain.")
    try:
        if not args.skip_toolchain_setup:
            run("rustup", "component", "add", "rustfmt", "clippy")
            run("rustup", "target", "add", TARGET, "x86_64-pc-windows-msvc")
        run("cargo", "fmt", "--manifest-path", MANIFEST, "--", "--check")
        run(
            "cargo",
            "test",
            "--locked",
            "--manifest-path",
            MANIFEST,
            "--target",
            TARGET,
            "--features",
            "installer",
        )
        for feature in ("installer", "fixture-engine", "native-svg"):
            run(
                "cargo",
                "clippy",
                "--locked",
                "--manifest-path",
                MANIFEST,
                "--target",
                TARGET,
                "--all-targets",
                "--features",
                feature,
                "--",
                "-D",
                "warnings",
            )
        run(
            sys.executable,
            "-W",
            "error",
            "-m",
            "py_compile",
            "ci.py",
            "build.py",
            "build_support.py",
            "dll_artifact.py",
            "test_dll_artifact.py",
            "build_preview.py",
            "desktop_helpers.py",
            "test_preview.py",
            "test_maintenance.py",
            "benchmark_preview.py",
            "test_toolkit.py",
        )
        run(sys.executable, "-B", "-m", "unittest", "test_dll_artifact")
        run(sys.executable, "test_toolkit.py")
        run(sys.executable, "build_preview.py", "--renderer", "svg")
        run(sys.executable, "build_preview.py", "--renderer", "raster")
        run(sys.executable, "build_preview.py", "--maintenance")
    except subprocess.CalledProcessError as error:
        print(f"Validation failed (exit code {error.returncode}).", file=sys.stderr)
        return error.returncode
    except FileNotFoundError as error:
        print(f"Required tool is missing: {error.filename}", file=sys.stderr)
        return 1
    print("All toolkit checks passed.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
