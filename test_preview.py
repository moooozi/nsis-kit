"""Opt-in desktop tests of harmless generic previews. Run suites serially."""

import argparse
import os
import subprocess
import sys
import time
import unittest
from pathlib import Path

from desktop_helpers import (
    CALLBACK,
    UI_CLASS,
    C,
    DesktopLease,
    LogFont,
    W,
    caption,
    click,
    g,
    monitor_work_areas,
    own_window,
    scale,
    screenshot,
    set_text,
    u,
    wait_for,
    windows,
)

ARTIFACTS = Path(__file__).resolve().parent / "artifacts"
RENDERER = "svg"
THEME = "dark"
EXE = ARTIFACTS / "NativeNSIS-preview-svg.exe"


class PreviewTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        lease = DesktopLease()
        cls.addClassCleanup(lease.close)

    def setUp(self):
        self.assertTrue(EXE.is_file(), "Run build.py first")
        self.baseline = set(windows(UI_CLASS))
        self.dialog_baseline = set(windows("#32770"))
        env = os.environ.copy()
        env["NATIVE_INSTALLER_THEME"] = THEME
        self.expected_renderer = 1 if RENDERER == "svg" else 2
        if self._testMethodName == "test_forced_svg_fallback" and RENDERER == "svg":
            env["NATIVE_INSTALLER_FORCE_ICO"] = "1"
            self.expected_renderer = 3
        else:
            env.pop("NATIVE_INSTALLER_FORCE_ICO", None)
        self.process = subprocess.Popen([str(EXE)], env=env)
        self.addCleanup(self.stop_preview)

        def startup():
            for dialog in windows("#32770"):
                if (
                    not own_window(dialog, self.process.pid)
                    or caption(dialog) != "Setup - Example App (NSIS preview)"
                ):
                    continue
                labels = []

                @CALLBACK
                def visit(child, _, labels=labels):
                    labels.append(caption(child))
                    return True

                u.EnumChildWindows(dialog, visit, 0)
                if any(
                    "Unable to create the preview UI" in value
                    or "Unable to load the preview UI" in value
                    for value in labels
                ):
                    click(dialog, 1)
                    raise AssertionError("Preview startup failed: " + " ".join(labels))
            return next(
                (w for w in windows(UI_CLASS) if own_window(w, self.process.pid)), None
            )

        self.window = wait_for(startup, "preview window")
        self.pid = W.DWORD()
        u.GetWindowThreadProcessId.argtypes = [W.HWND, C.POINTER(W.DWORD)]
        u.GetWindowThreadProcessId(self.window, C.byref(self.pid))

    def stop_preview(self):
        for hwnd in windows(UI_CLASS):
            if own_window(hwnd, self.process.pid):
                u.PostMessageW(hwnd, 0x10, 0, 0)
        try:
            self.process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            # Popen owns the exact process handle; no PID/name-based tree kill.
            self.process.kill()
            self.process.wait(timeout=5)

    def assert_layout(self, window):
        dpi = u.GetDpiForWindow(window)
        self.assertEqual(
            u.GetPropW(window, "NativeInstaller.NsisLogoRenderer"),
            self.expected_renderer,
            "SVG checks must not silently pass using the ICO fallback",
        )
        client = W.RECT()
        u.GetClientRect(window, C.byref(client))
        self.assertEqual(
            (client.right, client.bottom), (scale(640, dpi), scale(320, dpi))
        )
        for id, bounds in [
            (101, (58, 174, 430, 20)),
            (102, (44, 134, 240, 18)),
            (100, (516, 166, 80, 34)),
            (103, (44, 248, 544, 18)),
            (1, (458, 272, 80, 34)),
            (2, (548, 272, 80, 34)),
        ]:
            control = u.GetDlgItem(window, id)
            if not control:
                continue
            rect = W.RECT()
            u.GetWindowRect(control, C.byref(rect))
            u.MapWindowPoints(
                None, window, C.cast(C.byref(rect), C.POINTER(W.POINT)), 2
            )
            x, y, width, height = bounds
            if id == 1 and caption(control) == "Close":
                x = 548  # Completion uses the rightmost footer slot.
            self.assertEqual(
                (rect.left, rect.top, rect.right - rect.left, rect.bottom - rect.top),
                tuple(scale(v, dpi) for v in (x, y, width, height)),
            )
            self.assertGreaterEqual(rect.left, 0)
            self.assertLessEqual(rect.right, client.right)
            self.assertLessEqual(rect.bottom, client.bottom)
            font = LogFont()
            handle = u.SendMessageW(control, 0x31, 0, 0)  # WM_GETFONT
            self.assertTrue(g.GetObjectW(handle, C.sizeof(font), C.byref(font)))
            self.assertEqual(font.height, -((12 * dpi + 48) // 96))
        return dpi

    def test_selected_logo_renderer(self):
        self.assertEqual(
            u.GetPropW(self.window, "NativeInstaller.NsisLogoRenderer"),
            self.expected_renderer,
        )

    def test_forced_svg_fallback(self):
        if RENDERER != "svg":
            self.skipTest("SVG-only fallback test")
        self.assert_layout(self.window)
        screenshot(self.window, ARTIFACTS / "nsis-preview-svg-fallback.png")

    def test_per_monitor_v2_initial_layout(self):
        self.assertTrue(
            u.AreDpiAwarenessContextsEqual(
                u.GetWindowDpiAwarenessContext(self.window), C.c_void_p(-4)
            )
        )
        self.assert_layout(self.window)

    def test_monitor_moves_relayout_selection_and_completion(self):
        path = r"C:\Preview DPI\Example App"
        edit = u.GetDlgItem(self.window, 101)
        set_text(edit, path)
        u.SendMessageW(edit, 0xB1, 2, 7)  # EM_SETSEL
        seen = set()
        for _ in range(2):
            for area in monitor_work_areas():
                self.assertTrue(
                    u.SetWindowPos(
                        self.window, None, area.left + 30, area.top + 30, 0, 0, 0x15
                    )
                )
                u.UpdateWindow(self.window)
                seen.add(self.assert_layout(self.window))
                self.assertEqual(caption(edit), path)
                self.assertEqual(
                    u.SendMessageW(edit, 0xB0, 0, 0), 2 | (7 << 16)
                )  # EM_GETSEL
                dpi = u.GetDpiForWindow(self.window)
                screenshot(self.window, ARTIFACTS / f"nsis-preview-dpi-{dpi}.png")
        click(self.window, 1)
        finished = wait_for(
            lambda: next(
                (
                    w
                    for w in windows(UI_CLASS)
                    if w not in self.baseline and not u.GetDlgItem(w, 101)
                ),
                None,
            ),
            "completion page",
        )
        for area in monitor_work_areas():
            u.SetWindowPos(finished, None, area.left + 30, area.top + 30, 0, 0, 0x15)
            u.UpdateWindow(finished)
            self.assert_layout(finished)
        click(finished, 1)
        self.assertEqual(self.process.wait(timeout=10), 0)
        if len(seen) < 2:
            self.skipTest(
                "Layout passed, but multiple physical monitor DPIs were not available"
            )

    def test_hover_repaints_on_transition_not_every_mouse_move(self):
        button = u.GetDlgItem(self.window, 1)
        previous, rect = W.POINT(), W.RECT()
        u.GetCursorPos(C.byref(previous))
        self.addCleanup(u.SetCursorPos, previous.x, previous.y)
        u.GetWindowRect(button, C.byref(rect))
        # TrackMouseEvent checks the real pointer: posting synthetic moves with
        # the pointer elsewhere immediately generates a legitimate mouse-leave.
        point = W.POINT((rect.left + rect.right) // 2, (rect.top + rect.bottom) // 2)
        u.SetForegroundWindow(self.window)
        u.SetCursorPos(point.x, point.y)
        time.sleep(0.1)
        if u.WindowFromPoint(point) != button:
            self.skipTest(
                "The preview button is occluded on this desktop; TrackMouseEvent cannot test stationary hover"
            )
        u.SendMessageW(button, 0x2A3, 0, 0)  # WM_MOUSELEAVE
        u.UpdateWindow(button)
        u.SendMessageW(button, 0x200, 0, 8 | (8 << 16))
        u.UpdateWindow(button)
        for x in range(9, 30):
            u.SendMessageW(button, 0x200, 0, x | (8 << 16))
            self.assertFalse(
                u.GetUpdateRect(button, None, False),
                "Stationary hover must not repaint",
            )
        screenshot(self.window, ARTIFACTS / "nsis-preview-hover.png")
        u.SendMessageW(button, 0x2A3, 0, 0)
        u.UpdateWindow(button)

    def test_button_edges_have_antialiased_pixels(self):
        u.UpdateWindow(self.window)
        width, height, pixels = screenshot(
            self.window, ARTIFACTS / "nsis-preview-aa.png"
        )
        window, button = W.RECT(), W.RECT()
        u.GetWindowRect(self.window, C.byref(window))
        u.GetWindowRect(u.GetDlgItem(self.window, 1), C.byref(button))
        blends = 0
        for y in range(button.top - window.top, button.top - window.top + 3):
            for x in range(button.left - window.left, button.right - window.left):
                i = ((height - 1 - y) * width + x) * 4
                blue, green, red = pixels[i : i + 3]
                if (
                    THEME == "dark"
                    and 0 < red < 23
                    and 23 < green < 185
                    and 23 < blue < 232
                ) or (
                    THEME == "light"
                    and 0 < red < 245
                    and 185 < green < 245
                    and 232 < blue < 245
                ):
                    blends += 1
        self.assertGreater(
            blends, 0, "Rounded edges should blend cyan into the background"
        )

    def test_cancel_and_capture(self):
        program_files = Path(os.environ.get("ProgramW6432", os.environ["ProgramFiles"]))
        self.assertEqual(
            caption(u.GetDlgItem(self.window, 101)), str(program_files / "Example App")
        )
        screenshot(self.window, ARTIFACTS / "nsis-preview.png")
        click(self.window, 2)
        self.assertEqual(self.process.wait(timeout=10), 1)

    def test_install_runs_nsis_fixture_then_completes(self):
        # This deliberately nonexistent Unicode destination must remain untouched.
        destination = ARTIFACTS / "预览-only-do-not-create"
        self.assertFalse(destination.exists())
        set_text(u.GetDlgItem(self.window, 101), str(destination))
        click(self.window, 1)
        finished = wait_for(
            lambda: next(
                (
                    w
                    for w in windows(UI_CLASS)
                    if w not in self.baseline and not u.GetDlgItem(w, 101)
                ),
                None,
            ),
            "completion page",
        )
        screenshot(finished, ARTIFACTS / "nsis-preview-complete.png")
        click(finished, 1)
        self.assertEqual(self.process.wait(timeout=10), 0)
        self.assertFalse(destination.exists())

    def test_browse_cancel_preserves_typed_path(self):
        path = r"C:\Preview only\Example App"
        set_text(u.GetDlgItem(self.window, 101), path)
        previous = set(windows("#32770"))
        click(self.window, 100)
        dialog = wait_for(
            lambda: next((w for w in windows("#32770") if w not in previous), None),
            "folder browser",
        )
        click(dialog, 2)
        wait_for(lambda: dialog not in windows("#32770"), "folder browser close")
        self.assertEqual(caption(u.GetDlgItem(self.window, 101)), path)
        click(self.window, 2)
        self.assertEqual(self.process.wait(timeout=10), 1)

    def test_relative_path_is_rejected(self):
        set_text(u.GetDlgItem(self.window, 101), "relative-path")
        click(self.window, 1)
        wait_for(
            lambda: caption(u.GetDlgItem(self.window, 103)), "inline path validation"
        )
        self.assertEqual(
            caption(u.GetDlgItem(self.window, 103)),
            "Enter an absolute installation folder path.",
        )
        self.assertTrue(u.GetDlgItem(self.window, 101))
        click(self.window, 2)
        self.assertEqual(self.process.wait(timeout=10), 1)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(add_help=False)
    parser.add_argument("--renderer", choices=["raster", "svg"], default="svg")
    parser.add_argument("--theme", choices=["light", "dark"], default="dark")
    args, remaining = parser.parse_known_args()
    RENDERER = args.renderer
    THEME = args.theme
    EXE = ARTIFACTS / f"NativeNSIS-preview-{RENDERER}.exe"
    unittest.main(argv=[sys.argv[0], *remaining], verbosity=2)
