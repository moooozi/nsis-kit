; Each generated uninstaller owns its version's ordinary NSIS removal steps.
!macro UninstallCheck
  ${If} $Failure != ""
    Call un.Fail
  ${EndIf}
!macroend

Function un.Fail
  MessageBox MB_OK|MB_ICONSTOP "$Failure" /SD IDOK
  SetErrorLevel 2
  Quit
FunctionEnd

Function un.Initialize
  native_nsis_ui::InterfaceVersion /NOUNLOAD
  Pop $0
  ${If} $0 != 2
    StrCpy $Failure "The uninstaller scripts and native plugin have incompatible interfaces. Rebuild them together."
    Call un.Fail
  ${EndIf}
  SetRegView 64
  SetShellVarContext all
  Call un.CheckInstallLocation
  ReadRegStr $0 HKLM "${APP_KEY}" "InstallLocation"
  ReadRegStr $1 HKLM "${APP_KEY}" "${APP_MARKER}"
  ${If} $0 != $INSTDIR
  ${OrIf} $1 != "${APP_MARKER_VALUE}"
    StrCpy $Failure "This uninstaller does not match the registered ${APP_NAME} installation."
    Call un.Fail
  ${EndIf}
  native_nsis_ui::UninstallOptions /NOUNLOAD
  Pop $UninstallSilent
  SetSilent normal
  ${If} $UninstallSilent == 1
    SetSilent silent
  ${EndIf}
  native_nsis_ui::IsElevated /NOUNLOAD
  Pop $0
  ${If} $0 != 1
    StrCpy $3 ""
    ${If} $UninstallSilent == 1
      StrCpy $3 "/S"
    ${EndIf}
    ; NSIS has already relocated normal uninstall.exe invocations to temp.
    native_nsis_ui::Execute /NOUNLOAD "$EXEPATH" "$3 _?=$INSTDIR"
    Pop $1
    Pop $0
    ${If} $1 != ""
      MessageBox MB_OK|MB_ICONSTOP "$1" /SD IDOK
    ${EndIf}
    SetErrorLevel $0
    Quit
  ${EndIf}
FunctionEnd

!macro DefineNativeRemoval PREFIX
Function ${PREFIX}RemoveNativeInstallation
  SetOutPath "$TEMP"
  !insertmacro BeforeUninstall "${PREFIX}"
  ClearErrors
  !insertmacro DeleteFiles
  !insertmacro OwnedFiles DeleteOwnedFile
  Delete "$SMPROGRAMS\${APP_NAME}.lnk"
  ${If} ${Errors}
    StrCpy $Failure "Some ${APP_NAME} files could not be removed."
    Call ${PREFIX}Fail
  ${EndIf}
  ; Retain the uninstaller if payload deletion failed, so removal can be retried.
  ; An explicitly in-place _?= caller removes the mapped image after it exits.
  ${If} $EXEPATH != "$INSTDIR\uninstall.exe"
    Delete "$INSTDIR\uninstall.exe"
    ${If} ${Errors}
      StrCpy $Failure "The ${APP_NAME} uninstaller could not be removed."
      Call ${PREFIX}Fail
    ${EndIf}
  ${EndIf}
  ClearErrors
  DeleteRegKey HKLM "${APP_KEY}"
  ${If} ${Errors}
    StrCpy $Failure "${APP_NAME} registration could not be removed. Retry with administrator permission."
    Call ${PREFIX}Fail
  ${EndIf}
  ; Non-recursive removal preserves files that do not belong to this installer.
  !insertmacro RemoveDirectories
  RMDir "$INSTDIR"
FunctionEnd
!macroend
!insertmacro DefineNativeRemoval "un."

Section "Uninstall"
  Call un.RemoveNativeInstallation
  SetAutoClose true
  SetErrorLevel 0
SectionEnd
