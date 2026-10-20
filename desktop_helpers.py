"""Shared Windows desktop-test helpers; no product or preview dependencies."""
import ctypes as C
from ctypes import wintypes as W
import struct
import time
import zlib

UI_CLASS = "NativeNSISInstaller"
u = C.WinDLL("user32", use_last_error=True)
g = C.WinDLL("gdi32", use_last_error=True)
u.SetProcessDpiAwarenessContext.argtypes = [C.c_void_p]
u.SetProcessDpiAwarenessContext(C.c_void_p(-4))
CALLBACK = C.WINFUNCTYPE(W.BOOL, W.HWND, W.LPARAM)
u.EnumWindows.argtypes = [CALLBACK, W.LPARAM]
u.EnumChildWindows.argtypes = [W.HWND, CALLBACK, W.LPARAM]
u.GetClassNameW.argtypes = [W.HWND, W.LPWSTR, C.c_int]
u.GetWindowTextW.argtypes = [W.HWND, W.LPWSTR, C.c_int]
u.GetDlgItem.argtypes = [W.HWND, C.c_int]
u.GetDlgItem.restype = W.HWND
u.PostMessageW.argtypes = [W.HWND, W.UINT, W.WPARAM, W.LPARAM]
u.SendMessageW.argtypes = [W.HWND, W.UINT, W.WPARAM, W.LPARAM]
u.SendMessageW.restype = C.c_ssize_t
u.SetWindowTextW.argtypes = [W.HWND, W.LPCWSTR]
u.GetWindowRect.argtypes = [W.HWND, C.POINTER(W.RECT)]
u.IsWindowVisible.argtypes = [W.HWND]
u.GetDC.argtypes = [W.HWND]
u.GetDC.restype = W.HDC
u.ReleaseDC.argtypes = [W.HWND, W.HDC]
u.PrintWindow.argtypes = [W.HWND, W.HDC, W.UINT]
g.CreateCompatibleDC.argtypes = [W.HDC]
g.CreateCompatibleDC.restype = W.HDC
g.CreateCompatibleBitmap.argtypes = [W.HDC, C.c_int, C.c_int]
g.CreateCompatibleBitmap.restype = W.HBITMAP
g.SelectObject.argtypes = [W.HDC, W.HGDIOBJ]
g.SelectObject.restype = W.HGDIOBJ
g.DeleteObject.argtypes = [W.HGDIOBJ]
g.DeleteDC.argtypes = [W.HDC]
g.GetDIBits.argtypes = [W.HDC, W.HBITMAP, W.UINT, W.UINT, C.c_void_p, C.c_void_p, W.UINT]
u.GetPropW.argtypes = [W.HWND, W.LPCWSTR]
u.GetPropW.restype = W.HANDLE
u.GetDpiForWindow.argtypes = [W.HWND]
u.GetDpiForWindow.restype = W.UINT
u.GetClientRect.argtypes = [W.HWND, C.POINTER(W.RECT)]
u.SetWindowPos.argtypes = [W.HWND, W.HWND, C.c_int, C.c_int, C.c_int, C.c_int, W.UINT]
u.MapWindowPoints.argtypes = [W.HWND, W.HWND, C.POINTER(W.POINT), W.UINT]
u.UpdateWindow.argtypes = [W.HWND]
u.GetUpdateRect.argtypes = [W.HWND, C.POINTER(W.RECT), W.BOOL]
u.GetCursorPos.argtypes = [C.POINTER(W.POINT)]
u.SetCursorPos.argtypes = [C.c_int, C.c_int]
u.SetForegroundWindow.argtypes = [W.HWND]
u.WindowFromPoint.argtypes = [W.POINT]
u.WindowFromPoint.restype = W.HWND
u.GetWindowDpiAwarenessContext.argtypes = [W.HWND]
u.GetWindowDpiAwarenessContext.restype = C.c_void_p
u.AreDpiAwarenessContextsEqual.argtypes = [C.c_void_p, C.c_void_p]
MONITOR_CALLBACK = C.WINFUNCTYPE(W.BOOL, W.HMONITOR, W.HDC, C.POINTER(W.RECT), W.LPARAM)
u.EnumDisplayMonitors.argtypes = [W.HDC, C.POINTER(W.RECT), MONITOR_CALLBACK, W.LPARAM]
u.GetMonitorInfoW.argtypes = [W.HMONITOR, C.c_void_p]


class MonitorInfo(C.Structure):
    _fields_ = [("size", W.DWORD), ("monitor", W.RECT), ("work", W.RECT), ("flags", W.DWORD)]


class LogFont(C.Structure):
    _fields_ = [("height", W.LONG), ("width", W.LONG), ("escapement", W.LONG),
                ("orientation", W.LONG), ("weight", W.LONG),
                ("italic", W.BYTE), ("underline", W.BYTE), ("strike", W.BYTE),
                ("charset", W.BYTE), ("out", W.BYTE), ("clip", W.BYTE),
                ("quality", W.BYTE), ("pitch", W.BYTE), ("face", W.WCHAR * 32)]


g.GetObjectW.argtypes = [W.HGDIOBJ, C.c_int, C.c_void_p]


def monitor_work_areas():
    result = []

    @MONITOR_CALLBACK
    def visit(monitor, _dc, _rect, _param):
        info = MonitorInfo(size=C.sizeof(MonitorInfo))
        if u.GetMonitorInfoW(monitor, C.byref(info)):
            result.append(info.work)
        return True

    u.EnumDisplayMonitors(None, None, visit, 0)
    return result


def scale(value, dpi):
    return (value * dpi + 48) // 96


def windows(class_name):
    found = []

    @CALLBACK
    def visit(hwnd, _):
        name = C.create_unicode_buffer(256)
        u.GetClassNameW(hwnd, name, len(name))
        if name.value == class_name and u.IsWindowVisible(hwnd):
            found.append(hwnd)
        return True

    u.EnumWindows(visit, 0)
    return found


def wait_for(predicate, description):
    deadline = time.monotonic() + 12
    while time.monotonic() < deadline:
        result = predicate()
        if result:
            return result
        time.sleep(0.05)
    raise AssertionError("Timed out: " + description)


def caption(hwnd):
    text = C.create_unicode_buffer(1024)
    u.SendMessageW(hwnd, 0xD, len(text), C.addressof(text))
    return text.value


def set_text(hwnd, text):
    # Cross-process controls require WM_SETTEXT; SetWindowText only updates the
    # cached window caption and does not exercise the edit control's buffer.
    value = C.create_unicode_buffer(text)
    u.SendMessageW(hwnd, 0xC, 0, C.addressof(value))


def click(hwnd, control):
    # Post WM_COMMAND so modal Browse/error dialogs cannot block the test thread.
    u.PostMessageW(hwnd, 0x111, control, u.GetDlgItem(hwnd, control) or 0)


def screenshot(hwnd, path):
    """PrintWindow capture of just our window, never the user's desktop."""
    rect = W.RECT()
    u.GetWindowRect(hwnd, C.byref(rect))
    width, height = rect.right - rect.left, rect.bottom - rect.top
    dc = u.GetDC(hwnd)
    memory = g.CreateCompatibleDC(dc)
    bitmap = g.CreateCompatibleBitmap(dc, width, height)
    previous = g.SelectObject(memory, bitmap)
    try:
        if not u.PrintWindow(hwnd, memory, 2):
            raise AssertionError("PrintWindow failed")
        g.SelectObject(memory, previous)
        header = struct.pack("<IiiHHIIiiII", 40, width, height, 1, 32, 0, width * height * 4, 0, 0, 0, 0)
        info = C.create_string_buffer(header)
        pixels = C.create_string_buffer(width * height * 4)
        if not g.GetDIBits(memory, bitmap, 0, height, pixels, info, 0):
            raise AssertionError("GetDIBits failed")
        rows = []
        for y in reversed(range(height)):
            row = pixels.raw[y * width * 4:(y + 1) * width * 4]
            rgb = bytearray(width * 3)
            rgb[0::3], rgb[1::3], rgb[2::3] = row[2::4], row[1::4], row[0::4]
            rows.append(b"\0" + rgb)

        def chunk(kind, data):
            return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))

        path.write_bytes(b"\x89PNG\r\n\x1a\n"
                         + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0))
                         + chunk(b"IDAT", zlib.compress(b"".join(rows))) + chunk(b"IEND", b""))
        return width, height, pixels.raw
    finally:
        g.SelectObject(memory, previous)
        g.DeleteObject(bitmap)
        g.DeleteDC(memory)
        u.ReleaseDC(hwnd, dc)


def own_window(hwnd, pid):
    actual = W.DWORD()
    u.GetWindowThreadProcessId.argtypes = [W.HWND, C.POINTER(W.DWORD)]
    u.GetWindowThreadProcessId(hwnd, C.byref(actual))
    return actual.value == pid


class DesktopLease:
    """Reject concurrent cursor/focus test suites in the same Windows session."""
    kernel = C.WinDLL("kernel32", use_last_error=True)
    kernel.CreateMutexW.argtypes = [C.c_void_p, W.BOOL, W.LPCWSTR]
    kernel.CreateMutexW.restype = W.HANDLE
    kernel.CloseHandle.argtypes = [W.HANDLE]

    def __init__(self):
        C.set_last_error(0)
        self.handle = self.kernel.CreateMutexW(None, False, "Local\\NativeInstaller.NsisDesktopValidation")
        status = C.get_last_error()
        if not self.handle or status == 183:
            self.close()
            raise RuntimeError("Another NSIS desktop test suite is active; run desktop tests serially")

    def close(self):
        if self.handle:
            self.kernel.CloseHandle(self.handle)
            self.handle = None
