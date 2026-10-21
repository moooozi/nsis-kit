"""Opt-in desktop tests of the harmless maintenance preview, never the real installer."""

import os
import subprocess
import unittest
from pathlib import Path

from desktop_helpers import (
    UI_CLASS,
    DesktopLease,
    caption,
    click,
    own_window,
    screenshot,
    u,
    wait_for,
    windows,
)

ARTIFACTS = Path(__file__).resolve().parent / "artifacts"
EXE = ARTIFACTS / "NativeNSIS-maintenance-preview.exe"


class MaintenanceTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        lease = DesktopLease()
        cls.addClassCleanup(lease.close)

    @staticmethod
    def reap_fixture(process):
        if process.poll() is None:
            process.kill()
        process.wait(timeout=10)

    def preview(self, theme):
        self.assertTrue(EXE.is_file(), "Build with --maintenance first")

        process = subprocess.Popen(
            [str(EXE)], env={**os.environ, "NATIVE_INSTALLER_THEME": theme}
        )
        self.addCleanup(self.reap_fixture, process)
        window = wait_for(
            lambda: next(
                (w for w in windows(UI_CLASS) if own_window(w, process.pid)), None
            ),
            "maintenance fixture",
        )
        self.assertEqual(caption(window), "Setup - Example App (NSIS preview)")
        self.assertFalse(u.GetDlgItem(window, 101))
        self.assertEqual(caption(u.GetDlgItem(window, 104)), "&Uninstall")
        self.assertEqual(caption(u.GetDlgItem(window, 105)), "&Reinstall")
        self.assertEqual(caption(u.GetDlgItem(window, 1)), "&Launch")
        return process, window

    def test_launch_and_both_themes(self):
        for theme in ["light", "dark"]:
            with self.subTest(theme=theme):
                process, window = self.preview(theme)
                screenshot(window, ARTIFACTS / f"nsis-maintenance-{theme}.png")
                click(window, 1)
                self.assertEqual(process.wait(timeout=10), 10)

    def test_button_text_padding_across_monitor_dpis(self):
        import desktop_helpers as desktop

        desktop.g.GetTextExtentPoint32W.argtypes = [
            desktop.W.HDC,
            desktop.W.LPCWSTR,
            desktop.C.c_int,
            desktop.C.POINTER(desktop.W.SIZE),
        ]
        desktop.g.CreateFontIndirectW.argtypes = [desktop.C.POINTER(desktop.LogFont)]
        desktop.g.CreateFontIndirectW.restype = desktop.W.HFONT
        process, window = self.preview("dark")
        seen = set()
        for area in desktop.monitor_work_areas():
            self.assertTrue(
                u.SetWindowPos(window, None, area.left + 30, area.top + 30, 0, 0, 0x15)
            )
            u.UpdateWindow(window)
            dpi = u.GetDpiForWindow(window)
            seen.add(dpi)
            for control in [104, 105, 1]:
                button = u.GetDlgItem(window, control)
                rect = desktop.W.RECT()
                u.GetClientRect(button, desktop.C.byref(rect))
                self.assertEqual(
                    (rect.right, rect.bottom),
                    (desktop.scale(80, dpi), desktop.scale(34, dpi)),
                )
                # GDI fonts belong to their process. Recreate the control's
                # actual LOGFONT locally rather than selecting its foreign HFONT.
                font = desktop.LogFont()
                self.assertTrue(
                    desktop.g.GetObjectW(
                        u.SendMessageW(button, 0x31, 0, 0),
                        desktop.C.sizeof(font),
                        desktop.C.byref(font),
                    )
                )
                self.assertEqual(font.height, -desktop.scale(12, dpi))
                local_font = desktop.g.CreateFontIndirectW(desktop.C.byref(font))
                self.assertTrue(local_font)
                dc = u.GetDC(button)
                previous = desktop.g.SelectObject(dc, local_font)
                self.assertTrue(previous)
                try:
                    label = caption(button).replace("&", "")
                    extent = desktop.W.SIZE()
                    self.assertTrue(
                        desktop.g.GetTextExtentPoint32W(
                            dc, label, len(label), desktop.C.byref(extent)
                        )
                    )
                    self.assertGreaterEqual(
                        rect.right - extent.cx, desktop.scale(20, dpi)
                    )
                    self.assertGreaterEqual(
                        rect.bottom - extent.cy, desktop.scale(12, dpi)
                    )
                finally:
                    desktop.g.SelectObject(dc, previous)
                    u.ReleaseDC(button, dc)
                    desktop.g.DeleteObject(local_font)
            screenshot(window, ARTIFACTS / f"nsis-maintenance-dpi-{dpi}.png")
        click(window, 1)
        self.assertEqual(process.wait(timeout=10), 10)
        if len(seen) < 2:
            self.skipTest(
                "Button padding passed, but multiple physical monitor DPIs were not available"
            )

    def test_uninstall_and_reinstall_only_return_actions(self):
        for control, code in [(104, 12), (105, 11)]:
            with self.subTest(control=control):
                process, window = self.preview("light")
                click(window, control)
                self.assertEqual(process.wait(timeout=10), code)

    def test_escape_is_cancel_not_reinstall(self):
        process, window = self.preview("dark")
        # IsDialogMessage translates Escape to IDCANCEL even without a Cancel button.
        u.PostMessageW(window, 0x100, 0x1B, 0)
        self.assertEqual(process.wait(timeout=10), 1)


if __name__ == "__main__":
    unittest.main(verbosity=2)
