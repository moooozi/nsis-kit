; Consumer defines APP_* settings and optional hooks before including this file.
Unicode true
RequestExecutionLevel user
ManifestDPIAware true
ManifestDPIAwareness PerMonitorV2,PerMonitor
ManifestSupportedOS Win10
SetCompressor /SOLID lzma
CRCCheck force
AllowSkipFiles off
!include "LogicLib.nsh"
!include "FileFunc.nsh"
!include "WordFunc.nsh"
!include "TextFunc.nsh"
!include "x64.nsh"
!include "WinVer.nsh"
!ifndef VERSION
  !error "VERSION is required"
!endif
!ifndef PAYLOAD_EXE
  !error "PAYLOAD_EXE is required"
!endif
!ifndef PLUGIN_DIRECTORY
  !error "PLUGIN_DIRECTORY is required; use the toolkit builder"
!endif
!ifndef APP_MARKER
  !define APP_MARKER "NativeInstaller"
!endif
!ifndef APP_MARKER_VALUE
  !define APP_MARKER_VALUE "native-nsis-v1"
!endif
!ifndef APP_SUCCESS
  !define APP_SUCCESS "${APP_NAME} has been installed."
!endif
!include "${__FILEDIR__}\hooks.nsh"
Name "${APP_NAME}"
Caption "Setup - ${APP_NAME} v${VERSION}"
OutFile "${OUTPUT}"
InstallDir "${APP_DIRECTORY}"
!ifdef APP_ICON
  Icon "${APP_ICON}"
  UninstallIcon "${APP_ICON}"
!endif
!addplugindir /x86-unicode "${PLUGIN_DIRECTORY}"
VIProductVersion "${VERSION}.0"
VIAddVersionKey /LANG=1033 "ProductName" "${APP_NAME}"
VIAddVersionKey /LANG=1033 "FileDescription" "${APP_NAME} Setup"
VIAddVersionKey /LANG=1033 "FileVersion" "${VERSION}"
VIAddVersionKey /LANG=1033 "LegalCopyright" "${APP_PUBLISHER}"
Page instfiles
UninstPage uninstConfirm
UninstPage instfiles
AutoCloseWindow true
ShowInstDetails show
ShowUninstDetails show
SilentUnInstall normal
Var Failure
Var Automatic
Var Option
Var OptionCount
Var StopOptions
Var WorkerArguments
Var RestartApp
Var ExistingKey
Var ExistingKind
Var OldUninstaller
Var Action
Var ExistingVersion
Var Worker
Var UIWindow
Var UIActivate
Var UninstallSilent
Var OwnershipSupported
Var OwnedHandle
Var OwnedLine
Var OwnedPath
Var OwnedKeep
Var OwnedWalk
Var OwnedIndex
Var OwnedComponent
Var OwnedHasApp
Var OwnedApp
Var OwnedHasUninstaller
Var OwnedRemaining
Var OwnedCount
!include "${__FILEDIR__}\runtime.nsh"
!include "${__FILEDIR__}\options.nsh"
!include "${__FILEDIR__}\detection.nsh"
!include "${__FILEDIR__}\ownership.nsh"
!include "${__FILEDIR__}\install.nsh"
!include "${__FILEDIR__}\uninstall.nsh"
Function .onInit
  Call InitializeSetup
FunctionEnd
Function un.onInit
  Call un.Initialize
FunctionEnd
