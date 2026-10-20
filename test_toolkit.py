"""Headless tests of a separate consumer using only private HKCU/temp fixtures.

The fixture plugin uses ShellExecute open, never runas. No real application,
production installer, Program Files or real uninstall key is accessed.
"""
import ctypes
from ctypes import wintypes
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import time
import unittest
import uuid
import winreg

from build_support import HERE, build_plugin, find_makensis, verify_unelevated_manifest


class ToolkitTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        temporary = tempfile.TemporaryDirectory(prefix="native-installer-fixture-")
        cls.addClassCleanup(temporary.cleanup)
        cls.base = Path(temporary.name)
        cls.directory = cls.base / "Example App"
        cls.key = rf"Software\NativeInstallerFixture\{uuid.uuid4().hex}"
        cls.exe = cls.base / "example-fixture.exe"
        cls.uninstaller = cls.base / "relocated-uninstaller.exe"
        cls.addClassCleanup(cls.remove_registration)
        engine = cls.base / "engine"
        engine.mkdir()
        for source in HERE.glob("*.nsh"):
            text = source.read_text(encoding="utf-8").replace("HKLM", "HKCU")
            text = text.replace("SetShellVarContext all", "SetShellVarContext current")
            text = text.replace(r"$SMPROGRAMS\${APP_NAME}.lnk", r"$INSTDIR\example.lnk")
            if source.name == "runtime.nsh":
                text = text.replace('Function Fail\n', f'''Function Fail
  FileOpen $8 "{cls.base / 'failure.txt'}" w
  FileWriteUTF16LE $8 "$Failure"
  FileClose $8
''')
                text = text.replace('  native_nsis_ui::Launch /NOUNLOAD "$INSTDIR"\n  Pop $Failure',
                                    '  FileOpen $8 "$INSTDIR\\launch-requested" w\n  FileClose $8\n  StrCpy $Failure ""')
            (engine / source.name).write_text(text, encoding="utf-8")
        source = (HERE / "examples/desktop/setup.nsi").read_text(encoding="utf-8")
        source = source.replace(r"$PROGRAMFILES64\Example App", str(cls.directory))
        source = source.replace(r"Software\Microsoft\Windows\CurrentVersion\Uninstall\ExampleApp", cls.key)
        source = source.replace(r"${__FILEDIR__}\..\..\ui\examples\logo.svg", str(HERE / "ui/examples/logo.svg"))
        source = source.replace(r"${__FILEDIR__}\..\..\setup.nsh", str(engine / "setup.nsh"))
        script = cls.base / "setup.nsi"
        script.write_text(source, encoding="utf-8")
        payload = cls.base / "payload.rs"
        payload.write_text("fn main() {}\n", encoding="utf-8")
        application = cls.base / "arbitrary-payload-name.exe"
        subprocess.run(["rustc", "--edition=2024", "--target", "x86_64-pc-windows-msvc",
                        "-O", str(payload), "-o", str(application)], check=True, timeout=60)
        dll = build_plugin(HERE / "ui/target/fixture", features="fixture-engine")
        subprocess.run([find_makensis(), "-INPUTCHARSET", "UTF8", "-WX", "-V2", "-DVERSION=1.2.3",
                        f"-DPAYLOAD_EXE={application}", f"-DOUTPUT={cls.exe}",
                        f"-DPLUGIN_DIRECTORY={dll.parent}", str(script)], check=True, timeout=60)
        verify_unelevated_manifest(cls.exe)
        # A separate consumer supplies extra payload, integration copy and removal
        # hooks without changing the shared DLL or its NSIS implementation.
        cls.hooked = cls.base / "hooked-fixture.exe"
        extra = cls.base / "extra.txt"
        extra.write_text("consumer-owned payload", encoding="utf-8")
        hooks = r'''
Var ExampleValue
!macro InitializeOptions
  StrCpy $ExampleValue ""
!macroend
!macro ParseOption
  StrCpy $0 $Option 9
  ${If} $0 == "/EXAMPLE="
    StrCpy $ExampleValue $Option "" 9
  ${EndIf}
!macroend
!macro ValidateOptions
  ${If} $ExampleValue != ""
    StrCpy $WorkerArguments '$\"/EXAMPLE=$ExampleValue$\"'
  ${EndIf}
!macroend
!macro FixturePhase PHASE
  FileOpen $8 "PHASE_LOG" a
  FileSeek $8 0 END
  FileWrite $8 "${PHASE}$\r$\n"
  FileClose $8
!macroend
!macro PreflightInstall
  !insertmacro FixturePhase "preflight"
  ${If} $ExampleValue == "reject-preflight"
    StrCpy $Failure "Example App rejected preflight."
    Call Fail
  ${EndIf}
  FileOpen $8 "OPTION_LOG" w
  FileWriteUTF16LE $8 "$ExampleValue"
  FileClose $8
!macroend
!macro PrepareApplication
  !insertmacro FixturePhase "prepare"
!macroend
!macro BeforeReplace
  !insertmacro FixturePhase "replace"
!macroend
!macro InstallFiles
  SetOutPath "$INSTDIR\extras"
  File /oname=owned.txt "EXTRA_PAYLOAD"
  ${If} ${Errors}
    StrCpy $Failure "Example App data could not be installed."
    Call Fail
  ${EndIf}
!macroend
!macro AfterInstall
  !insertmacro FixturePhase "installed"
  !insertmacro Progress 85 "Preparing Example App data..."
!macroend
!macro BeforeUninstall PREFIX
  FileOpen $8 "$INSTDIR\uninstall-observed" w
  FileClose $8
!macroend
!macro AppOwnedFiles CALLBACK
  !insertmacro ${CALLBACK} "extras\owned.txt"
!macroend
!macro RemoveDirectories
  RMDir "$INSTDIR\extras"
!macroend
'''.replace("EXTRA_PAYLOAD", str(extra)).replace("PHASE_LOG", str(cls.base / "phases.txt"))
        hooks = hooks.replace("OPTION_LOG", str(cls.base / "received-option.txt"))
        script.write_text(hooks + source, encoding="utf-8")
        subprocess.run([find_makensis(), "-INPUTCHARSET", "UTF8", "-WX", "-V2", "-DVERSION=1.2.3",
                        f"-DPAYLOAD_EXE={application}", f"-DOUTPUT={cls.hooked}",
                        f"-DPLUGIN_DIRECTORY={dll.parent}", str(script)], check=True, timeout=60)

    @classmethod
    def remove_registration(cls):
        try:
            winreg.DeleteKeyEx(winreg.HKEY_CURRENT_USER, cls.key, winreg.KEY_WOW64_64KEY)
        except FileNotFoundError:
            pass

    def run_setup(self, *args, expected=0, installer=None):
        failure = self.base / "failure.txt"
        failure.unlink(missing_ok=True)
        command = subprocess.list2cmdline([str(installer or self.exe), "/S", *args]) + f" /D={self.directory}"
        result = subprocess.run(command, timeout=30)
        self.assertEqual(result.returncode, expected,
                         failure.read_text(encoding="utf-16-le") if failure.exists() else "")

    def setUp(self):
        self.remove_registration()
        # All test installs share the compiled fixture, but never installed state.
        self.assertEqual(self.directory.resolve().parent, self.base.resolve())
        if self.directory.exists():
            shutil.rmtree(self.directory)
        (self.base / "phases.txt").unlink(missing_ok=True)

    def installed_value(self, name):
        with winreg.OpenKey(winreg.HKEY_CURRENT_USER, self.key, 0,
                           winreg.KEY_READ | winreg.KEY_WOW64_64KEY) as key:
            return winreg.QueryValueEx(key, name)

    def write_ownership_value(self, name, value, kind=winreg.REG_MULTI_SZ):
        with winreg.OpenKey(winreg.HKEY_CURRENT_USER, self.key, 0,
                           winreg.KEY_WRITE | winreg.KEY_WOW64_64KEY) as key:
            winreg.SetValueEx(key, name, 0, kind, value)

    def assert_rejected_without_changes(self, executable_bytes, phases):
        self.run_setup("/AUTOMATIC", expected=2, installer=self.hooked)
        self.assertEqual((self.directory / "example.exe").read_bytes(), executable_bytes)
        self.assertEqual((self.base / "phases.txt").read_text(encoding="utf-8"), phases)
        self.assertFalse((self.directory / "uninstall-observed").exists())
        self.assertEqual(self.installed_value("FileOwnershipProtocol"), (4, winreg.REG_DWORD))

    def test_ownership_lists_are_relative_and_separate_from_registry_keys(self):
        self.run_setup(installer=self.hooked)
        self.assertEqual(self.installed_value("OwnedFiles"),
                         (["example.exe", "uninstall.exe", r"extras\owned.txt"], winreg.REG_MULTI_SZ))
        self.assertEqual(self.installed_value("OwnedRegistryKeys"),
                         (["HKCU:64\\" + self.key], winreg.REG_MULTI_SZ))

    def test_invalid_paths_and_junctions_fail_before_consumer_hooks(self):
        self.run_setup(installer=self.hooked)
        executable_bytes = (self.directory / "example.exe").read_bytes()
        phases = (self.base / "phases.txt").read_text(encoding="utf-8")
        paths = [r"..\..\outside.txt", r"resource\..\..\..\outside.txt",
                 r"resources\..\outside.txt", r".\outside.txt", r"C:\outside.txt",
                 r"C:outside.txt", r"\rooted.txt", r"\\server\share\outside.txt",
                 r"\\?\C:\outside.txt", r"resources\*.ico", r"resources\file:stream",
                 "resources/../outside.txt", "", "x" * 901, "file. ", "file.",
                 "file\r\n[RegistryKeys]", "bad\tfile", r"resource\\file.txt"]
        outside = self.base / "outside-owned-list"
        outside.mkdir()
        sentinel = outside / "keep.txt"
        sentinel.write_text("preserve", encoding="utf-8")
        junction = self.directory / "linked-directory"
        result = subprocess.run(["cmd", "/c", "mklink", "/J", str(junction), str(outside)],
                                capture_output=True, timeout=10)
        self.assertEqual(result.returncode, 0, "Unable to create the private junction fixture")
        self.addCleanup(junction.rmdir)
        paths.append(r"linked-directory\keep.txt")
        for path in paths:
            with self.subTest(path=path):
                self.write_ownership_value("OwnedFiles", ["example.exe", "uninstall.exe", path])
                self.assert_rejected_without_changes(executable_bytes, phases)
                self.assertIn("unsafe path" if path else "Registry string list",
                              (self.base / "failure.txt").read_text(encoding="utf-16-le"))
                self.assertEqual(sentinel.read_text(encoding="utf-8"), "preserve")

    def test_advertised_manifest_must_be_complete_and_typed(self):
        self.run_setup(installer=self.hooked)
        executable_bytes = (self.directory / "example.exe").read_bytes()
        phases = (self.base / "phases.txt").read_text(encoding="utf-8")
        cases = [("example.exe", winreg.REG_SZ), ([], winreg.REG_MULTI_SZ),
                 (["example.exe"], winreg.REG_MULTI_SZ),
                 (["unrelated.txt", "another.txt"], winreg.REG_MULTI_SZ),
                 (["uninstall.exe", "uninstall.exe"], winreg.REG_MULTI_SZ)]
        for value, kind in cases:
            with self.subTest(value=value, kind=kind):
                self.write_ownership_value("OwnedFiles", value, kind)
                self.assert_rejected_without_changes(executable_bytes, phases)
        with winreg.OpenKey(winreg.HKEY_CURRENT_USER, self.key, 0,
                           winreg.KEY_WRITE | winreg.KEY_WOW64_64KEY) as key:
            winreg.DeleteValue(key, "OwnedFiles")
        self.assert_rejected_without_changes(executable_bytes, phases)
        self.write_ownership_value("OwnedFiles", ["example.exe", "uninstall.exe"])
        for value, kind in [(["example.exe"], winreg.REG_MULTI_SZ),
                            (["HKCU:64\\Unrelated"], winreg.REG_MULTI_SZ),
                            ([], winreg.REG_MULTI_SZ), ("HKCU:64\\" + self.key, winreg.REG_SZ)]:
            with self.subTest(registry_value=value, kind=kind):
                self.write_ownership_value("OwnedRegistryKeys", value, kind)
                self.assert_rejected_without_changes(executable_bytes, phases)

    def test_large_unicode_manifest_cleans_obsolete_files_without_old_uninstaller(self):
        self.run_setup(installer=self.hooked)
        # A supported manifest is sufficient; the old uninstaller is never run.
        shutil.copyfile(self.directory / "example.exe", self.directory / "uninstall.exe")
        obsolete = self.directory / "old payload \u00e9\u4e2d\u6587.dat"
        obsolete.write_text("old owned payload", encoding="utf-8")
        sentinel = self.directory / "user-data.txt"
        sentinel.write_text("preserve", encoding="utf-8")
        self.write_ownership_value("OwnedFiles", ["example.exe", "uninstall.exe", obsolete.name,
                                   *[f"missing-{index:04}.dat" for index in range(1000)]])
        self.run_setup("/AUTOMATIC", installer=self.hooked)
        self.assertFalse(obsolete.exists())
        self.assertEqual(sentinel.read_text(encoding="utf-8"), "preserve")
        self.assertFalse((self.directory / "uninstall-observed").exists())
        self.assertEqual(self.installed_value("OwnedFiles")[0],
                         ["example.exe", "uninstall.exe", r"extras\owned.txt"])

    def test_upgrade_can_rename_the_main_executable(self):
        self.run_setup(installer=self.hooked)
        old_executable = self.directory / "previous-name.exe"
        (self.directory / "example.exe").rename(old_executable)
        self.write_ownership_value("DisplayIcon", str(old_executable), winreg.REG_SZ)
        self.write_ownership_value("OwnedFiles", [old_executable.name, "uninstall.exe", r"extras\owned.txt"])
        self.run_setup("/AUTOMATIC", installer=self.hooked)
        self.assertFalse(old_executable.exists())
        self.assertTrue((self.directory / "example.exe").is_file())
        self.assertFalse((self.directory / "uninstall-observed").exists())

    def test_invalid_installed_executable_identity_fails_before_shutdown(self):
        self.run_setup(installer=self.hooked)
        phases = self.base / "phases.txt"
        before = phases.read_bytes()
        executable = self.directory / "example.exe"
        payload = executable.read_bytes()
        for identity in [str(self.base / "outside.exe"),
                         str(self.directory) + r"\resources\..\example.exe",
                         str(self.directory / "not-in-manifest.exe"), ""]:
            with self.subTest(identity=identity):
                self.write_ownership_value("DisplayIcon", identity, winreg.REG_SZ)
                self.run_setup("/AUTOMATIC", installer=self.hooked, expected=2)
                self.assertEqual(phases.read_bytes(), before)
                self.assertEqual(executable.read_bytes(), payload)
                self.assertEqual(self.installed_value("FileOwnershipProtocol")[0], 4)

    def test_absent_or_unknown_ownership_protocol_runs_the_installed_uninstaller(self):
        self.run_setup()
        for protocol in [None, 1, 2, 3, 99]:
            with self.subTest(protocol=protocol):
                self.run_setup("/AUTOMATIC", installer=self.hooked)
                marker = self.directory / "uninstall-observed"
                marker.unlink(missing_ok=True)
                with winreg.OpenKey(winreg.HKEY_CURRENT_USER, self.key, 0,
                                   winreg.KEY_WRITE | winreg.KEY_WOW64_64KEY) as key:
                    if protocol is None:
                        winreg.DeleteValue(key, "FileOwnershipProtocol")
                    else:
                        winreg.SetValueEx(key, "FileOwnershipProtocol", 0, winreg.REG_DWORD, protocol)
                self.run_setup("/AUTOMATIC")
                self.assertTrue(marker.is_file())
                self.assertTrue((self.directory / "example.exe").is_file())
                self.assertEqual(self.installed_value("FileOwnershipProtocol"), (4, winreg.REG_DWORD))

    def test_consumer_hook_sequence_distinguishes_preflight_shutdown_and_replacement(self):
        phases = self.base / "phases.txt"
        self.run_setup(installer=self.hooked)
        self.assertEqual(phases.read_text(encoding="utf-8").splitlines(),
                         ["preflight", "prepare", "installed"])
        phases.unlink()
        self.run_setup("/AUTOMATIC", installer=self.hooked)
        self.assertEqual(phases.read_text(encoding="utf-8").splitlines(),
                         ["preflight", "prepare", "replace", "installed"])
        self.assertFalse((self.directory / "uninstall-observed").exists())

    def test_failed_app_preflight_does_not_request_shutdown_or_replace_payload(self):
        self.run_setup(installer=self.hooked)
        phases = self.base / "phases.txt"
        phases.unlink()
        executable = self.directory / "example.exe"
        payload = executable.read_bytes()
        self.run_setup("/AUTOMATIC", "/EXAMPLE=reject-preflight", installer=self.hooked, expected=2)
        self.assertEqual(phases.read_text(encoding="utf-8").splitlines(), ["preflight"])
        self.assertEqual(executable.read_bytes(), payload)
        self.assertEqual(self.installed_value("FileOwnershipProtocol")[0], 4)

    def test_interactive_and_silent_brokers_forward_custom_consumer_arguments(self):
        # Send messages only to this fixture's exact PID/class. This exercises
        # the real broker without moving the pointer, focus or another window.
        user = ctypes.WinDLL("user32", use_last_error=True)
        callback_type = ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.HWND, wintypes.LPARAM)
        user.EnumWindows.argtypes = [callback_type, wintypes.LPARAM]
        user.EnumWindows.restype = wintypes.BOOL
        user.GetWindowThreadProcessId.argtypes = [wintypes.HWND, ctypes.POINTER(wintypes.DWORD)]
        user.GetWindowThreadProcessId.restype = wintypes.DWORD
        user.GetClassNameW.argtypes = [wintypes.HWND, wintypes.LPWSTR, ctypes.c_int]
        user.GetClassNameW.restype = ctypes.c_int
        user.GetPropW.argtypes = [wintypes.HWND, wintypes.LPCWSTR]
        user.GetPropW.restype = wintypes.HANDLE
        user.PostMessageW.argtypes = [wintypes.HWND, wintypes.UINT, wintypes.WPARAM, wintypes.LPARAM]
        user.PostMessageW.restype = wintypes.BOOL
        value = "custom value \u00e9\u4e2d\u6587"
        argument = "/EXAMPLE=" + value
        command = subprocess.list2cmdline([str(self.hooked), argument]) + f" /D={self.directory}"
        process = subprocess.Popen(command)

        def stop_fixture():
            if process.poll() is None:
                process.terminate()
                process.wait(timeout=10)

        self.addCleanup(stop_fixture)

        def wait_for(predicate, description):
            deadline = time.monotonic() + 30
            while time.monotonic() < deadline:
                result = predicate()
                if result:
                    return result
                self.assertIsNone(process.poll(), f"Fixture exited while waiting for {description}")
                time.sleep(0.05)
            self.fail(f"Timed out waiting for {description}")

        def find_window():
            found = []

            @callback_type
            def visit(window, _):
                pid = wintypes.DWORD()
                user.GetWindowThreadProcessId(window, ctypes.byref(pid))
                if pid.value == process.pid:
                    name = ctypes.create_unicode_buffer(256)
                    user.GetClassNameW(window, name, len(name))
                    if name.value == "NativeNSISInstaller":
                        found.append(window)
                return True

            self.assertTrue(user.EnumWindows(visit, 0))
            return found[0] if found else None

        window = wait_for(find_window, "fixture installer window")
        self.assertTrue(user.PostMessageW(window, 0x111, 1, 0))  # WM_COMMAND / Install
        wait_for(lambda: user.GetPropW(window, "NativeInstaller.NsisOperation") == 2,
                 "interactive worker completion")
        self.assertTrue(user.PostMessageW(window, 0x111, 1, 0))  # Close completed page
        self.assertEqual(process.wait(timeout=10), 0)
        received = self.base / "received-option.txt"
        self.assertEqual(received.read_text(encoding="utf-16-le"), value)
        received.unlink()
        self.run_setup("/AUTOMATIC", argument, installer=self.hooked)
        self.assertEqual(received.read_text(encoding="utf-16-le"), value)

    def test_desktop_consumer_lifecycle_without_product_hooks(self):
        self.run_setup()
        executable = self.directory / "example.exe"
        self.assertTrue(executable.is_file())
        self.assertTrue((self.directory / "example.lnk").is_file())
        with winreg.OpenKey(winreg.HKEY_CURRENT_USER, self.key, 0,
                           winreg.KEY_READ | winreg.KEY_WOW64_64KEY) as key:
            self.assertEqual(winreg.QueryValueEx(key, "DisplayName")[0], "Example App")
            self.assertEqual(winreg.QueryValueEx(key, "DisplayVersion")[0], "1.2.3")
        old_uninstaller = self.directory / "uninstall.exe"
        old_uninstaller_bytes = old_uninstaller.read_bytes()
        # A zero exit code alone is insufficient if removal did not happen.
        # The dummy application simply exits successfully without removing files.
        shutil.copyfile(executable, old_uninstaller)
        # Unsupported versions must still use and verify the old uninstaller.
        with winreg.OpenKey(winreg.HKEY_CURRENT_USER, self.key, 0,
                           winreg.KEY_WRITE | winreg.KEY_WOW64_64KEY) as key:
            winreg.SetValueEx(key, "FileOwnershipProtocol", 0, winreg.REG_DWORD, 99)
        self.run_setup("/AUTOMATIC", expected=2)
        self.assertEqual(old_uninstaller.read_bytes(), executable.read_bytes())
        old_uninstaller.write_bytes(old_uninstaller_bytes)
        with winreg.OpenKey(winreg.HKEY_CURRENT_USER, self.key, 0,
                           winreg.KEY_WRITE | winreg.KEY_WOW64_64KEY) as key:
            winreg.SetValueEx(key, "FileOwnershipProtocol", 0, winreg.REG_DWORD, 4)
        self.run_setup("/AUTOMATIC")
        self.run_setup("/ACTION=reinstall", expected=2)
        self.run_setup("/WORKER", "/ACTION=unknown", expected=2)
        self.run_setup("/WORKER", "/UIHWND=invalid", expected=2)
        self.run_setup("/WORKER", "/ACTION=reinstall")
        self.assertFalse((self.directory / "launch-requested").exists())
        self.run_setup("/WORKER", "/ACTION=reinstall", "/RESTARTAPP")
        self.assertTrue((self.directory / "launch-requested").is_file())
        with winreg.OpenKey(winreg.HKEY_CURRENT_USER, self.key, 0,
                           winreg.KEY_WRITE | winreg.KEY_WOW64_64KEY) as key:
            winreg.SetValueEx(key, "DisplayVersion", 0, winreg.REG_SZ, "9.0.0")
        self.run_setup("/AUTOMATIC", expected=2)
        sentinel = self.directory / "user.txt"
        sentinel.write_text("preserve")
        shutil.copyfile(self.directory / "uninstall.exe", self.uninstaller)
        command = subprocess.list2cmdline([str(self.uninstaller), "/S"]) + f" _?={self.directory}"
        self.assertEqual(subprocess.run(command, timeout=30).returncode, 0)
        self.assertFalse(executable.exists())
        self.assertFalse((self.directory / "uninstall.exe").exists())
        self.assertEqual(sentinel.read_text(), "preserve")
        with self.assertRaises(FileNotFoundError):
            winreg.OpenKey(winreg.HKEY_CURRENT_USER, self.key, 0,
                           winreg.KEY_READ | winreg.KEY_WOW64_64KEY)
        self.run_setup(installer=self.hooked)
        owned = self.directory / "extras/owned.txt"
        self.assertEqual(owned.read_text(), "consumer-owned payload")
        unknown = self.directory / "extras/user.txt"
        unknown.write_text("preserve extra user data")
        # The new installer doesn't know about extras/owned.txt. It learns about
        # the obsolete payload from the registered ownership manifest.
        self.run_setup("/AUTOMATIC")
        self.assertFalse(owned.exists())
        self.assertFalse((self.directory / "uninstall-observed").exists())
        self.assertEqual(unknown.read_text(), "preserve extra user data")
        # An unknown protocol version must use the old uninstall path, never
        # invoke a protocol command that the old executable may not understand.
        self.run_setup("/AUTOMATIC", installer=self.hooked)
        (self.directory / "uninstall-observed").unlink(missing_ok=True)
        with winreg.OpenKey(winreg.HKEY_CURRENT_USER, self.key, 0,
                           winreg.KEY_WRITE | winreg.KEY_WOW64_64KEY) as key:
            winreg.SetValueEx(key, "FileOwnershipProtocol", 0, winreg.REG_DWORD, 99)
        self.run_setup("/AUTOMATIC")
        self.assertFalse(owned.exists())
        self.assertTrue((self.directory / "uninstall-observed").exists())
        self.assertEqual(sentinel.read_text(), "preserve")
        self.assertTrue(executable.is_file())
        # Maintenance removal must likewise use the installed version's hooks.
        self.run_setup("/AUTOMATIC", installer=self.hooked)
        self.assertTrue(owned.exists())
        (self.directory / "uninstall-observed").unlink()
        self.run_setup("/WORKER", "/ACTION=uninstall")
        self.assertFalse(owned.exists())
        self.assertFalse(executable.exists())
        self.assertEqual(unknown.read_text(), "preserve extra user data")
        self.assertTrue((self.directory / "uninstall-observed").is_file())


if __name__ == "__main__":
    if os.name != "nt":
        raise SystemExit("This fixture requires Windows.")
    unittest.main()
