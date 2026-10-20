Unicode true
RequestExecutionLevel user
ManifestDPIAware true
ManifestDPIAwareness PerMonitorV2,PerMonitor
ManifestSupportedOS Win10
SetCompressor /SOLID lzma
Name "Native NSIS UI preview"
Caption "Setup - Example App (NSIS preview)"
OutFile "${OUTPUT}"
!ifdef PREVIEW_ICON
  Icon "${PREVIEW_ICON}"
!endif
!addplugindir /x86-unicode "${PLUGIN_DIRECTORY}"
!include "LogicLib.nsh"

# No application files or registration: even /S and /D cannot target an install.
SilentInstall silent
AutoCloseWindow true
ShowInstDetails nevershow
Var PreviewPath

Function .onInit
  InitPluginsDir
  !ifdef NATIVE_SVG
    File /oname=$PLUGINSDIR\brand.svg "${PREVIEW_LOGO}"
    StrCpy $0 "$PLUGINSDIR\brand.svg"
  !else
    StrCpy $0 ""
  !endif
  native_nsis_ui::Configure /NOUNLOAD "Example App" "A harmless preview of the native installer UI." "example.exe" "Example App preview completed successfully." "$0"
  Pop $0
  ${If} $0 != ""
    MessageBox MB_OK|MB_ICONSTOP "$0"
    SetErrorLevel 2
    Quit
  ${EndIf}
  !ifdef MAINTENANCE_PREVIEW
    ; Visual-only fixture: actions return exit codes, never launch/remove/install.
    native_nsis_ui::Select /NOUNLOAD "maintenance-preview" "$PROGRAMFILES64\Example App"
    Pop $0
    Pop $PreviewPath
    ${If} $0 == "launch"
      SetErrorLevel 10
    ${ElseIf} $0 == "reinstall"
      SetErrorLevel 11
    ${ElseIf} $0 == "uninstall"
      SetErrorLevel 12
    ${Else}
      SetErrorLevel 1
    ${EndIf}
    Quit
  !endif

  ClearErrors
  native_nsis_ui::Choose /NOUNLOAD "$PROGRAMFILES64\Example App" "@103"
  IfErrors plugin_load_failed
  Pop $0
  Pop $PreviewPath
  ${If} $0 == "cancel"
    SetErrorLevel 1
    Quit
  ${ElseIf} $0 != "install"
    MessageBox MB_OK|MB_ICONSTOP "Unable to create the preview UI.$\r$\n$0"
    SetErrorLevel 2
    Quit
  ${EndIf}
  Return
plugin_load_failed:
  MessageBox MB_OK|MB_ICONSTOP "Unable to load the preview UI plug-in. Rebuild using build.py to check its DLL dependencies."
  SetErrorLevel 2
  Quit
FunctionEnd

Section "Harmless temporary fixture"
  ClearErrors
  SetOutPath "$PLUGINSDIR"
  File "${__FILEDIR__}\fixture.txt"
  ${If} ${Errors}
    native_nsis_ui::Complete /NOUNLOAD "error" "@103"
    SetErrorLevel 2
    Quit
  ${EndIf}
  native_nsis_ui::Complete /NOUNLOAD "success" "@103"
SectionEnd
