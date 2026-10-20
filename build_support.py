"""Reusable Windows build checks; never execute installer/application payloads."""
from contextlib import contextmanager
from pathlib import Path
import os
import shutil
import subprocess

HERE = Path(__file__).resolve().parent


@contextmanager
def build_lock(target_dir=None):
    """Preview and production builds share a DLL output; never interleave them."""
    import msvcrt
    target = Path(target_dir or HERE / "ui/target").resolve()
    target.mkdir(parents=True, exist_ok=True)
    with (target / "native-nsis-build.lock").open("a+b") as lock:
        lock.seek(0)
        if not lock.read(1):
            lock.write(b"\0")
            lock.flush()
        lock.seek(0)
        try:
            msvcrt.locking(lock.fileno(), msvcrt.LK_NBLCK, 1)
        except OSError as error:
            raise SystemExit("Another NSIS build is active. Retry after it finishes.") from error
        try:
            yield
        finally:
            lock.seek(0)
            msvcrt.locking(lock.fileno(), msvcrt.LK_UNLCK, 1)


def find_makensis():
    compiler = shutil.which("makensis")
    if not compiler:
        candidate = Path(os.environ.get("ProgramFiles(x86)", "C:/Program Files (x86)")) / "NSIS/makensis.exe"
        if candidate.is_file():
            compiler = str(candidate)
    if not compiler:
        raise SystemExit("Install stock NSIS 3 (Unicode).")
    return compiler


def build_plugin(target_dir=None, features="installer"):
    target_dir = Path(target_dir or HERE / "ui/target").resolve()
    common = ["--locked", "--release", "--target", "i686-pc-windows-msvc",
              "--manifest-path", str(HERE / "ui/Cargo.toml"), "--target-dir", str(target_dir)]
    if features:
        common += ["--features", features]
    subprocess.run(["cargo", "build", *common], check=True, timeout=180)
    dll = target_dir / "i686-pc-windows-msvc/release/native_nsis_ui.dll"
    subprocess.run(["cargo", "run", *common, "--example", "check_plugin", "--", str(dll),
                    *(["--production"] if "installer" in features.split(",") else [])], check=True, timeout=60)
    return dll


def verify_payload_architecture(executable):
    with executable.open("rb") as image:
        header = image.read(64)
        if len(header) != 64 or header[:2] != b"MZ":
            raise RuntimeError("Application payload is not a Windows PE executable")
        offset = int.from_bytes(header[60:64], "little")
        if offset > 1024 * 1024:
            raise RuntimeError("Application payload PE header is out of bounds")
        image.seek(offset)
        if image.read(6) != b"PE\0\0\x64\x86":
            raise RuntimeError("The installer requires an x64 application payload")


def verify_unelevated_manifest(executable):
    """Map the EXE as data only; never execute installer code."""
    import ctypes as C
    from ctypes import wintypes as W
    import xml.etree.ElementTree as ET
    kernel = C.WinDLL("kernel32", use_last_error=True)
    kernel.LoadLibraryExW.argtypes = [W.LPCWSTR, W.HANDLE, W.DWORD]
    kernel.LoadLibraryExW.restype = W.HMODULE
    kernel.FindResourceW.argtypes = [W.HMODULE, C.c_void_p, C.c_void_p]
    kernel.FindResourceW.restype = W.HANDLE
    kernel.SizeofResource.argtypes = [W.HMODULE, W.HANDLE]
    kernel.SizeofResource.restype = W.DWORD
    kernel.LoadResource.argtypes = [W.HMODULE, W.HANDLE]
    kernel.LoadResource.restype = W.HANDLE
    kernel.LockResource.argtypes = [W.HANDLE]
    kernel.LockResource.restype = C.c_void_p
    kernel.FreeLibrary.argtypes = [W.HMODULE]
    module = kernel.LoadLibraryExW(str(executable), None, 2)
    if not module:
        raise OSError(C.get_last_error(), "Unable to inspect installer manifest")
    try:
        resource = kernel.FindResourceW(module, C.c_void_p(1), C.c_void_p(24))
        size = kernel.SizeofResource(module, resource) if resource else 0
        pointer = kernel.LockResource(kernel.LoadResource(module, resource)) if size else None
        if not pointer:
            raise OSError(C.get_last_error(), "Installer manifest is missing")
        manifest = ET.fromstring(C.string_at(pointer, size))
        levels = [item.get("level") for item in manifest.iter() if item.tag.endswith("requestedExecutionLevel")]
        if levels != ["asInvoker"]:
            raise RuntimeError(f"Installer must start unelevated; manifest levels: {levels}")
    finally:
        kernel.FreeLibrary(module)
    print("Verified installer manifest: asInvoker (no startup elevation)")
