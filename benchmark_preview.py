"""Opt-in, warm-start A/B measurement of only the harmless preview executables."""

import json
import statistics
import time
from pathlib import Path

import test_preview as preview

ROOT = Path(__file__).resolve().parent
OUTPUT = ROOT / "artifacts/nsis-svg-evaluation"


def launch(renderer, capture=False):
    preview.RENDERER = renderer
    preview.EXE = ROOT / f"artifacts/NativeNSIS-preview-{renderer}.exe"
    case = preview.PreviewTests("test_selected_logo_renderer")
    try:
        start = time.perf_counter()
        case.setUp()
        elapsed = (time.perf_counter() - start) * 1000
        case.test_selected_logo_renderer()
        if capture:
            preview.screenshot(case.window, OUTPUT / f"preview-{renderer}.png")
        return elapsed
    finally:
        case.doCleanups()


def main():
    OUTPUT.mkdir(parents=True, exist_ok=True)
    lease = preview.DesktopLease()
    original_wait = preview.wait_for

    def fast_wait(predicate, description):
        deadline = time.monotonic() + 12
        while time.monotonic() < deadline:
            result = predicate()
            if result:
                return result
            time.sleep(0.002)
        raise AssertionError("Timed out: " + description)

    preview.wait_for = fast_wait
    try:
        # Warm both executables/DLLs before alternating measured launches.
        for renderer in ["raster", "svg"]:
            launch(renderer, capture=True)
        measurements = {"raster": [], "svg": []}
        for _ in range(7):
            for renderer in ["raster", "svg"]:
                measurements[renderer].append(launch(renderer))
    finally:
        preview.wait_for = original_wait
        lease.close()
    report = {
        "method": "7 alternating warm launches per renderer; spawn to visible preview; 2ms polling",
        "renderers": {
            renderer: {
                **json.loads(
                    (
                        ROOT / f"artifacts/NativeNSIS-preview-{renderer}.build.json"
                    ).read_text()
                ),
                "startup_ms": values,
                "startup_median_ms": statistics.median(values),
                "startup_min_ms": min(values),
                "startup_max_ms": max(values),
            }
            for renderer, values in measurements.items()
        },
    }
    (OUTPUT / "startup-comparison.json").write_text(json.dumps(report, indent=2))
    print(json.dumps(report, indent=2))


if __name__ == "__main__":
    main()
