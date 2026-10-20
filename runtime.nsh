; Small shared checks and presentation helpers. Installation stays in NSIS.
!macro HandoffForeground
  ${If} $UIActivate == 1
    native_nsis_ui::HandoffForeground /NOUNLOAD "$UIWindow"
    StrCpy $UIActivate 0
  ${EndIf}
!macroend

!macro Progress PERCENT TEXT
  native_nsis_ui::Report /NOUNLOAD "$UIWindow" "${PERCENT}" "${TEXT}"
!macroend

!macro CheckResult
  ${If} $Failure != ""
    Call Fail
  ${EndIf}
!macroend

; This toolkit supports one consumer-configured, machine-wide destination.
!macro DefineLocationCheck PREFIX
Function ${PREFIX}CheckInstallLocation
  StrCmp "$INSTDIR" "${APP_DIRECTORY}" location_ok
  StrCpy $Failure "This installer uses ${APP_DIRECTORY}. Remove an older custom-location installation with its original uninstaller before installing here."
  Call ${PREFIX}Fail
location_ok:
FunctionEnd
!macroend
!insertmacro DefineLocationCheck ""
!insertmacro DefineLocationCheck "un."

Function Fail
  !insertmacro HandoffForeground
  DetailPrint "$Failure"
  !insertmacro Progress 1000 "Error: $Failure"
  MessageBox MB_OK|MB_ICONSTOP "$Failure" /SD IDOK
  SetErrorLevel 2
  Quit
FunctionEnd

Function ConfigureUI
  native_nsis_ui::InterfaceVersion /NOUNLOAD
  Pop $0
  ${If} $0 != 2
    StrCpy $Failure "The installer scripts and native plugin have incompatible interfaces. Rebuild them together."
    Call Fail
  ${EndIf}
  InitPluginsDir
  !ifdef APP_LOGO
    File /oname=$PLUGINSDIR\brand.svg "${APP_LOGO}"
    StrCpy $0 "$PLUGINSDIR\brand.svg"
  !else
    StrCpy $0 ""
  !endif
  native_nsis_ui::Configure /NOUNLOAD "${APP_NAME}" "${APP_DESCRIPTION}" "${APP_EXE}" "${APP_SUCCESS}" "$0"
  Pop $Failure
  !insertmacro CheckResult
  ; Use NSIS's section size directly, in KiB.
  SectionGetSize 0 $1
  native_nsis_ui::RequiredSpace /NOUNLOAD "$1"
  Pop $Failure
  !insertmacro CheckResult
FunctionEnd

Function Launch
  native_nsis_ui::Launch /NOUNLOAD "$INSTDIR"
  Pop $Failure
  !insertmacro CheckResult
FunctionEnd
