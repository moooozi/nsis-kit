# Native NSIS toolkit

> [!NOTE]
> This repository is vibe coded.

A reusable Rust/Win32 Unicode UI and ordinary NSIS installer/uninstaller for
Windows 10/11 x64 machine-wide desktop applications. The toolkit is a standalone
repository with its own build scripts, examples, tests and CI. Consumers supply
application payload, branding and integration hooks. `examples/desktop/setup.nsi`
provides a minimal consumer with no custom lifecycle hooks.

NSIS owns extraction, registration, shortcuts and explicit file removal. The
shared DLL owns presentation, elevation, worker observation and unelevated launch.
The [ownership protocol](#file-ownership-protocol) governs replacement and removal.
Failures stop without automatic rollback.
Concurrent installation/removal is unsupported. New installs use the configured
default destination; upgrades and maintenance use the registered installation
directory. `/D=` selects a destination for a new installation. Installations are
machine-wide and application payloads must be x64.

## Build an installer

To try the unmodified example with an existing x64 application executable:

```console
python path/to/toolkit/build.py --script path/to/toolkit/examples/desktop/setup.nsi --payload path/to/app.exe --version 1.2.3 --output path/to/ExampleApp-setup.exe
```

This packages the executable as `example.exe` with Example App branding. The
builder compiles the UI plugin and installer but does not launch either the app
or installer. For your own product, follow the configuration steps below.

Copy the desktop example into your project and adjust its `APP_*` defines and
toolkit include path. Supply these settings:

| Define | Meaning |
| --- | --- |
| `APP_NAME` | Display name; also the Start menu shortcut basename |
| `APP_EXE` | Application executable basename, including `.exe` |
| `APP_DIRECTORY` | Default machine-wide destination, normally under `$PROGRAMFILES64` |
| `APP_KEY` | Unique 64-bit HKLM uninstall key |
| `APP_PUBLISHER` | Publisher/version-resource attribution |
| `APP_DESCRIPTION` | Installation screen description |
| `APP_ICON` | Optional ICO used by NSIS and the ICO fallback |
| `APP_LOGO` | Optional SVG supplied by this consumer |
| `APP_SUCCESS` | Optional successful-installation text |
| `APP_MARKER`, `APP_MARKER_VALUE` | Optional registry ownership field/value |

Use Windows-valid filename characters in `APP_NAME` and `APP_EXE`. Use quoted
NSIS literals and NSIS escaping for your strings, rather than inserting untrusted
text. Keep the executable and installation directory appropriate for a
machine-wide application. Reserve `uninstall.exe` for the generated uninstaller.

For a configured consumer, set `--script` to its entry point in the build command.

Requires stock NSIS 3 Unicode, Python 3.11+, Rust/MSVC and the
`i686-pc-windows-msvc` / `x86_64-pc-windows-msvc` targets. The builder checks the x64
payload, x86 plugin exports, absence of fixture-only code, NSIS warnings and
`asInvoker` manifest. Versions have three numeric components in `0..65535`.
Use `--target-dir` for an isolated build cache. Builds
sharing the standard DLL output hold a build-time lock through NSIS compilation.
Signing and release publication belong to the consumer.

### Prebuilt production DLL

`.github/workflows/build-dll.yml` builds the production DLL on each push to `main`
and supports manual runs. It compiles the `installer` feature for
`i686-pc-windows-msvc`, loads the DLL in the export checker and rejects fixture
code. The result is published with build provenance as a prerelease named
`dll-<full-toolkit-commit>`, containing `native_nsis_ui.dll` and `plugin.json`.
Existing DLL releases are never overwritten and are not marked as the latest
stable release. Consumers use the exact toolkit commit, not a moving branch or
latest release; binaries remain available without Actions artifact expiry.

Download both files into a directory and pass `--plugin-artifact path/to/dll`
to the installer builder to skip plugin compilation. `dll_artifact.py` verifies
the toolkit revision, production features, target and SHA-256 checksum before use.
`python dll_artifact.py path/to/dll --verify` performs the same check separately.
Without `--plugin-artifact`, local builds continue to compile the plugin.

## Presentation and maintenance

The compact UI uses direct dimensions: a 640 by 320 DIP client, 80 by 34 DIP
buttons, 12 DIP body text and 24 DIP headings. Monitor DPI is the only scaling
factor for both controls and fonts.

The destination is a disabled field with muted text: it cannot receive focus,
accept edits or select text. The all-users explanation appears below it. The
footer displays NSIS's `SectionGetSize` value directly, rounded up to KB or MB
for presentation. No custom reserve or allowance is added. `RequiredSpace` passes
the NSIS KiB value to the shared DLL before its window opens.

SVG logos should provide a `viewBox`. The renderer fits that viewBox into the
logo area and retains the SVG's `preserveAspectRatio` setting.

The installer opens unelevated. Install/Reinstall/Uninstall request permission
through a same-EXE worker. Launch remains unelevated. The custom window persists
through permission, stage progress, errors and completion. Maintenance shows the
installed/setup versions and selects Upgrade, Reinstall or Downgrade wording.
It instructs users to close all app instances before replacement or uninstall.

## Lifecycle hooks

Define optional NSIS macros before including `setup.nsh`; defaults in `hooks.nsh`
are empty except the explicit error for unconfigured non-native removal. Consumers
supply integrations without requiring a particular app CLI or update protocol.

| Macro | Contract |
| --- | --- |
| `DiscoverPrevious` | Read-only discovery before routing; may update `$ExistingKey`, `$ExistingKind`, `$INSTDIR` |
| `RemovePrevious` | Remove a discovered non-native installation and verify completion |
| `PreflightInstall` | Read-only app prerequisites; reject unsupported state before shutdown |
| `PrepareApplication` | Request and wait for application shutdown, before any installation mutations |
| `AppOwnedFiles CALLBACK` | Enumerate every additional packaged file as a relative path using the supplied callback |
| `BeforeReplace` | Remove old integrations before ownership-aware replacement; the installed executable is available |
| `InstallFiles` | Extract additional product-owned payload and check errors |
| `AfterInstall` | Configure integrations after registration/uninstaller have been written |
| `BeforeUninstall PREFIX` | Remove integrations in the installed uninstaller; `PREFIX` is `un.` |
| `DeleteFiles` | Optional cleanup of runtime-created product files, beyond the enumerated payload; preserve the NSIS error flag |
| `RemoveDirectories` | Remove empty additional directories only |

Hooks run in the elevated worker except read-only discovery. On failure, set
`$Failure` and `Call Fail`, or `Call ${PREFIX}Fail` in the removal hook. Functions
needed by a hook must exist in the corresponding installer/uninstaller namespace.
Hooks are ordinary NSIS macros, not string commands supplied at runtime.

Application-specific progress labels are passed through `Progress`, not embedded
in the DLL. For example, a consumer can define this before including `setup.nsh`:

```nsis
!macro AfterInstall
  !insertmacro Progress 85 "Configuring Example App integrations..."
  ; Perform and check this application's integration work here.
!macroend
```

Choose monotonically increasing percentages within the stage being extended:
`PreflightInstall` and then `PrepareApplication` run after 5 (before either replacement path), `BeforeReplace`
after 20 for protocol-aware updates, `InstallFiles` after 40, and
`AfterInstall` after 75, before the shared final 100 report. Shared stage labels
remain product-neutral. `APP_DESCRIPTION` and `APP_SUCCESS` supply product copy.

Ownership validation precedes `PreflightInstall` and `PrepareApplication`.
The latter may communicate with the app but must not mutate the installation.
Explicit removal uses `BeforeUninstall`, which must also stop components that
prevent file deletion. All hooks stop on failure.

`BeforeReplace` is compiled into the incoming installer, whereas
`BeforeUninstall` belongs to the installed version. Consumers must provide
cleanup for integrations created by every installed version they support.
Share an app-owned cleanup helper between those hooks when appropriate. The
file ownership protocol does not track services, scheduled tasks, firewall rules
or arbitrary registry integrations; those remain consumer responsibilities.

The hooks used on each path are:

| Operation | Hook sequence after validation |
| --- | --- |
| Fresh install | `PreflightInstall` → `PrepareApplication` → `InstallFiles` → `AfterInstall` |
| Supported ownership replacement | `PreflightInstall` → `PrepareApplication` → `BeforeReplace` → obsolete-file cleanup → `InstallFiles` → `AfterInstall` |
| Native fallback replacement | `PreflightInstall` → `PrepareApplication` → installed uninstaller (its own removal hooks) → `InstallFiles` → `AfterInstall` |
| Non-native migration | `PreflightInstall` → `PrepareApplication` → `RemovePrevious` → `InstallFiles` → `AfterInstall` |
| Explicit removal | Installed uninstaller: `BeforeUninstall` → `DeleteFiles` → owned-file/shortcut/registration removal → `RemoveDirectories` |

Around these hooks, the core worker extracts the main executable and writes the
uninstaller/registration before `AfterInstall`, then commits ownership on success.
Discovery and option parsing also run unelevated and must perform no privileged work.

Useful toolkit variables available to hooks:

| Variable | Meaning / permitted use |
| --- | --- |
| `$INSTDIR` | Validated fixed destination; use it to resolve app-relative files |
| `$ExistingKey` | Installed registration key, or empty on a fresh install |
| `$ExistingKind` | `native`, `legacy`, or empty; `DiscoverPrevious` may supply discovery state |
| `$ExistingVersion` | Installed display version when discovered |
| `$Action` | Worker operation: `install`, `reinstall` or `uninstall`; version wording does not change this value |
| `$OwnershipSupported` | `1` when an installed manifest was accepted; otherwise `0` |
| `$Failure` | Error text; set it and immediately call the appropriate `Fail` function |
| `$Automatic`, `$RestartApp` | Option-hook outputs: unattended replacement policy and explicit launch request |
| `$WorkerArguments` | Validated consumer options to forward to the elevated worker |
| `$Option`, `$StopOptions` | Current parsed token and consumer request to stop option interpretation |

Use named consumer variables for persistent hook state. Toolkit helpers may
overwrite scratch registers `$0`–`$9`; preserve values needed by later hooks. The removal
hook's `PREFIX` selects `un.Fail` in the generated uninstaller, not a different
application command.

## File ownership protocol

The installed uninstaller embeds a list of its owned files. The same `OwnedFiles`
enumeration generates both the registry manifest and uninstall deletion instructions.
The main executable and `uninstall.exe` are included automatically. Consumers must
list **all additional packaged files** through `AppOwnedFiles`, for example:

```nsis
!macro InstallFiles
  ClearErrors
  SetOutPath "$INSTDIR\resources"
  File "path\to\example.dat"
  ${If} ${Errors}
    StrCpy $Failure "Example App data could not be installed."
    Call Fail
  ${EndIf}
!macroend

!macro AppOwnedFiles CALLBACK
  !insertmacro ${CALLBACK} "resources\example.dat"
!macroend

!macro RemoveDirectories
  RMDir "$INSTDIR\resources"
!macroend
```

`InstallFiles` extracts those files. Do not enumerate settings, user data,
wildcards, absolute paths, or directories as owned files. Use `RemoveDirectories`
for non-recursive cleanup of empty product directories. Generated file lists should
come from the same payload inputs as extraction, rather than directory snapshots.

After successful payload extraction and integration hooks, Setup publishes these
values in the 64-bit registry view under the consumer's `APP_KEY`:

| Value | Type | Meaning |
|---|---|---|
| `FileOwnershipProtocol` | `REG_DWORD` | `4`, written last to commit the manifest |
| `OwnedFiles` | `REG_MULTI_SZ` | File paths relative to the registered `InstallLocation` |
| `OwnedRegistryKeys` | `REG_MULTI_SZ` | Separate registry paths with explicit hive and view |

The new enumeration is validated before preflight or extraction. Publication occurs
after `AfterInstall` succeeds, with the protocol marker written last. A fresh failed
installation retains repair registration without advertising a complete manifest.

For protocol 4 installations, Setup reads the registry directly. It does not launch
the old uninstaller or consume stdout for discovery. Lists must have the correct
registry type, valid terminated UTF-16, at most 100,000 entries and at most 16 MiB
per value. Oversized entries are rejected rather than truncated through the NSIS
stack. The generic DLL supplies registry string-list I/O; ownership policy stays
in the NSIS scripts.

Setup validates every file entry, then retains a private snapshot before the
consumer's `PrepareApplication` handoff. Only canonical relative paths of at most 900
UTF-16 code units are accepted. Absolute, rooted, UNC and drive-relative paths,
`.`/`..` components anywhere, forward slashes, wildcards, alternate streams, empty
components, trailing dots/spaces and existing reparse-point components are rejected.
The registered location must match the consumer's fixed `APP_DIRECTORY`.
Paths are revalidated when the snapshot is consumed for obsolete-file removal.

`OwnedRegistryKeys` currently contains the toolkit's single registration key,
`HKLM:64\<APP_KEY>`. Setup validates this separately and refreshes it through
`RegisterInstallation`. Normal uninstall removes it through the registry API.
Additional application integrations remain the responsibility of hooks. Registry
entries never enter file deletion. An advertised v4 manifest that is missing,
malformed or unsafe stops Setup before handoff or mutation; it does not trigger
uninstall fallback.

Supported replacement deletes only old entries absent from the new list and prunes
their empty parent folders. Current owned files are overwritten; unowned user files
are preserved. Explicit uninstall uses its embedded file list, removes its shortcut
and registration, and removes only empty directories.

If the ownership capability is absent or its version is unsupported, Setup instead
copies the old uninstaller into its private temporary directory, runs it silently
with `_?=$INSTDIR`, waits for its process and verifies removal before extracting
new files. This avoids NSIS's asynchronous uninstaller self-relocation. The old
version's removal instructions are used, not the new version's file list.
A missing old uninstaller blocks replacement. If installation fails after fallback
removal, rerun Setup as a fresh installation; the old version is not restored.
Explicit Uninstall always runs the installed version's normal removal path.

The installed native `DisplayIcon` identifies the installed main executable as an
exact path below `InstallLocation`, matching the registration written by this
toolkit. Its validated relative path and `uninstall.exe` must appear in the old
list. The incoming executable may have a different name: the old executable is
then obsolete and removed through the same ownership rules. Invalid or missing
identity metadata fails before app shutdown, rather than weakening validation.

To inspect Example App's installed manifest, run this with Python on Windows:

```python
import winreg

path = r"Software\Microsoft\Windows\CurrentVersion\Uninstall\ExampleApp"
with winreg.OpenKey(winreg.HKEY_LOCAL_MACHINE, path, 0,
                    winreg.KEY_READ | winreg.KEY_WOW64_64KEY) as key:
    for name in ("FileOwnershipProtocol", "OwnedFiles", "OwnedRegistryKeys"):
        print(name, winreg.QueryValueEx(key, name)[0])
```

## Plugin interface and compatibility

The plugin is `native_nsis_ui.dll`. `Configure` takes product name, description,
executable basename, successful-installation text and optional extracted SVG path,
returning an empty string or an error. Configuration is per process, so every
frontend/worker initializes it. Assets are supplied at runtime; rebuilding the
DLL for a different product is unnecessary. ICO resources remain in the NSIS EXE.
Preview entry points also require configuration to use consumer branding.

`Run` / `Execute` / `Report` implement frontend-worker presentation. Success is
determined from the worker process exit code, never a progress message. Progress
is stage-based. Foreground, renderer and operation diagnostic properties use the
`NativeInstaller.Nsis*` prefix; the window class is `NativeNSISInstaller`.

`ConfigureMaintenance` accepts the NSIS-selected presentation action
(`upgrade`, `reinstall`, `downgrade`), installed version and setup version.
The same replacement worker handles all three; labels and progress headings
reflect the selected action.

Command-line policy lives in NSIS, not the UI. The DLL's `Tokenize` helper
only splits Windows arguments: pop the error string and count, then pop exactly
that many arguments in their original order. It interprets no flags.

The shared NSIS runtime supports `/AUTOMATIC` for unattended replacement of an
existing registration, with downgrade rejection, and `/RESTARTAPP` for an explicit
post-install launch. Install and Reinstall otherwise leave the application closed.
Terminal `/D=` follows NSIS semantics but must match the configured fixed path.

Consumer command-line extensions use three optional macros:

| Macro | Contract |
| --- | --- |
| `InitializeOptions` | Initialize consumer-owned option state in each process |
| `ParseOption` | Interpret `$Option`; set `$StopOptions=1` to end interpretation |
| `ValidateOptions` | Validate consumer state and set `$WorkerArguments` for forwarding |

The runtime drains all remaining tokens after interpretation stops, preserving the
NSIS stack. Worker arguments must be correctly quoted NSIS strings built from
validated consumer state. `NormalizeWorkerArguments` tokenizes and requotes them,
rejects reserved runtime options and rejects commands that would overflow the
NSIS string buffer. Both silent and interactive launch routes forward this opaque
state. The elevated worker parses and validates it again; the UI interprets no
consumer options. Set `$Automatic` and `$RestartApp` directly rather than putting
their reserved options in `$WorkerArguments`.
These hooks must not mutate the installation. No product updater protocol is built
into the toolkit.

`PrepareShutdown` is an optional OS helper taking an exact executable path,
complete event name and PID. It signals that event and waits up to 35 seconds for
the matching process to exit. Event naming, invocation and update policy belong
to the consumer.

`/WORKER`, `/ACTION=`, `/UIHWND=` and `/ACTIVATEUI` are internal worker arguments.
They do not bypass elevation or the configured destination/registration checks.
The installer script/plugin interface is version **2**, returned by
`InterfaceVersion` and checked at initialization. `Run` takes mode, directory,
restart flag, then normalized consumer arguments. `NormalizeWorkerArguments`
takes raw consumer arguments and directory, and returns canonical arguments then
an error (pop the error first). See source exports for the other stack contracts.
Build scripts and DLL together against interface 2.

## Validation

From the toolkit directory:

```console
python ci.py
```

The runner also works from another directory: `python path/to/toolkit/ci.py`.
It installs the required Rust components/targets through `rustup`, then stops at
the first failed check and returns a nonzero exit code. To use a prepared
toolchain without running those setup commands, use
`python ci.py --skip-toolchain-setup`. NSIS, Python, Rust and the Visual Studio C++
build tools must be installed. Child Python checks use the same Python
interpreter as the runner.

This checks formatting, unit tests, three Clippy feature configurations and
desktop-consumer lifecycle and ownership security fixtures, both with default hooks
and with consumer-supplied payload, options, progress text and removal hooks.
These cover unsafe paths/reparse points, malformed lists, executable renames,
fallback removal, hook ordering and custom argument forwarding. The fixture compiles real NSIS
sources with a private HKCU key, temp destination and compile-time `open` broker;
it never elevates, runs a real app or touches Program Files.
CI also compiles SVG, raster and maintenance previews without launching them.
Application-specific lifecycle tests belong in the consumer repository.
Accessibility and real UAC acceptance need desktop testing.

## Harmless UI previews

The reusable previews use Example App branding and the toolkit's own SVG.
They require no consumer checkout, Node tooling or app assets. Build them with:

```console
python build_preview.py --renderer svg
python build_preview.py --renderer raster
python build_preview.py --maintenance
```

Outputs are under this directory's ignored `artifacts/`: `NativeNSIS-preview-svg.exe`,
`NativeNSIS-preview-raster.exe` and `NativeNSIS-maintenance-preview.exe`.
Raster mode uses the NSIS executable's icon; `--icon path/to/icon.ico` supplies an
optional consumer icon. SVG mode accepts `--logo path/to/logo.svg`. All builds
verify the unelevated manifest and never launch their output.

These previews request no elevation, write no registration and never install,
uninstall or launch an app. The selection fixture extracts only `preview/fixture.txt`
into NSIS's private temporary plugin directory. Its editable destination and Browse
control are for testing; the real installer uses a fixed, disabled destination.
The maintenance fixture returns action exit codes only: Launch 10, Reinstall 11,
Uninstall 12, Cancel 1. The selected app path is never modified, including with
`/S` or `/D=`.

Run the optional desktop suites serially on an interactive Windows desktop:

```console
python test_preview.py --renderer svg --theme dark
python test_preview.py --renderer svg --theme light
python test_preview.py --renderer raster --theme dark
python test_maintenance.py
```

They cover rendering/fallback, layout, monitor DPI, editable path behavior,
completion, maintenance actions and cancellation. Screenshots capture only the
fixture window. `desktop_helpers.py` supplies shared Win32 helpers and a
`DesktopLease` that rejects overlapping suites. Hover tests briefly move the
pointer and restore it afterward; leave the desktop undisturbed during these runs.
Tests requiring multiple physical monitor DPIs may skip on a single-DPI desktop.
The hover repaint check has a known intermittent desktop timing failure; inspect
that result separately from the headless lifecycle and ownership checks.
These desktop tests are opt-in; CI compiles previews without opening their windows.

`python benchmark_preview.py` optionally compares warm SVG/raster preview startup
after both have been built. Raw SVG-render diagnostics are available through
`ui/examples/svg_probe.rs`; neither tool is part of installer behavior.

## Repository integration

Consumers can pin the toolkit as a Git submodule:

```console
git submodule add https://github.com/moooozi/nsis-kit.git packaging/nsis/toolkit
git submodule update --init --recursive
```

Commit the toolkit gitlink and `.gitmodules` together. Pin a reviewed commit or
tag, and publish toolkit commits before publishing a consumer reference to them.
CI checkouts must initialize submodules recursively. Keep application registration
identity, ownership data and supported updater-client contracts consistent when
updating the pinned toolkit. Build the configured consumer with `build.py` and
validate its app hooks alongside the toolkit tests.

## Uninstall behavior

Direct uninstall uses NSIS's native confirmation/progress pages and closes on
success. Cancellation is available before removal; `/S` skips confirmation.
`UninstallString` and `QuietUninstallString` contain the quoted
`$INSTDIR\uninstall.exe` path followed by `/S`, avoiding a second Windows confirmation. Direct launches
without `/S` are interactive. Automatic replacement explicitly uses `/S`.

## Troubleshooting

| Symptom | What to check |
| --- | --- |
| Missing compiler or linker | Install stock NSIS 3, Rust/MSVC and Visual Studio C++ build tools; rerun `python ci.py` |
| Script/plugin interface mismatch | Rebuild scripts and DLL together with `build.py`; do not reuse an older plugin DLL |
| Invalid or incomplete ownership list | Check typed registry values, installed `DisplayIcon`, and relative file names; a malformed supported manifest deliberately blocks replacement |
| App files cannot be replaced or removed | Close running app instances; app hooks must stop services or other app components before replacement/removal |
| Integration command fails | Check its exit code in the consumer hook, report `$Failure`, and rerun after correcting the cause; changes are not rolled back |
| Installed app uses a custom destination | Remove it with its original uninstaller before using this fixed-location toolkit |
