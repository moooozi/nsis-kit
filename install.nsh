; Standard NSIS extraction and registration with consumer lifecycle hooks.
Function RegisterInstallation
  ClearErrors
  WriteRegStr HKLM "${APP_KEY}" "DisplayName" "${APP_NAME}"
  WriteRegStr HKLM "${APP_KEY}" "DisplayVersion" "${VERSION}"
  WriteRegStr HKLM "${APP_KEY}" "Publisher" "${APP_PUBLISHER}"
  WriteRegStr HKLM "${APP_KEY}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKLM "${APP_KEY}" "DisplayIcon" "$INSTDIR\${APP_EXE}"
  WriteRegStr HKLM "${APP_KEY}" "UninstallString" '"$INSTDIR\uninstall.exe" /S'
  WriteRegStr HKLM "${APP_KEY}" "QuietUninstallString" '"$INSTDIR\uninstall.exe" /S'
  WriteRegStr HKLM "${APP_KEY}" "${APP_MARKER}" "${APP_MARKER_VALUE}"
  WriteRegDWORD HKLM "${APP_KEY}" "NoModify" 1
  WriteRegDWORD HKLM "${APP_KEY}" "NoRepair" 1
  CreateShortcut /NoWorkingDir "$SMPROGRAMS\${APP_NAME}.lnk" "$INSTDIR\${APP_EXE}"
  ${If} ${Errors}
    StrCpy $Failure "Unable to register ${APP_NAME}. Retry Setup with administrator permission to repair the installation."
    Call Fail
  ${EndIf}
FunctionEnd

; Run the installed version's own removal instructions, including files that the
; new version no longer ships. Copy it to temp and use _?= so ExecWait observes
; the actual uninstaller instead of NSIS's asynchronous self-relocation parent.
Function RemoveInstalledVersion
  InitPluginsDir
  SetOutPath "$TEMP"
  ClearErrors
  CopyFiles /SILENT "$INSTDIR\uninstall.exe" "$PLUGINSDIR\previous-uninstall.exe"
  ${If} ${Errors}
    StrCpy $Failure "Unable to prepare the previous uninstaller. Close other Setup windows and retry."
    Call Fail
  ${EndIf}
  ClearErrors
  ExecWait '"$PLUGINSDIR\previous-uninstall.exe" /S _?=$INSTDIR' $0
  ${If} ${Errors}
  ${OrIf} $0 != 0
    StrCpy $Failure "The previous uninstaller failed or was canceled. No new files have been installed."
    Call Fail
  ${EndIf}
  IfFileExists "$INSTDIR\${APP_EXE}" removal_incomplete
  IfFileExists "$INSTDIR\uninstall.exe" removal_incomplete
  ClearErrors
  ReadRegStr $0 HKLM "${APP_KEY}" "InstallLocation"
  ${If} $0 != ""
    Goto removal_incomplete
  ${EndIf}
  Return
removal_incomplete:
  StrCpy $Failure "The previous installation was not fully removed. Complete its removal before retrying Setup."
  Call Fail
FunctionEnd

Section "${APP_NAME}"
  !insertmacro HandoffForeground
  !insertmacro Progress 5 "Preparing ${APP_NAME}..."
  ${If} $Action == "uninstall"
    ${If} $ExistingKind == "native"
      Call RemoveInstalledVersion
    ${Else}
      !insertmacro RemovePrevious
    ${EndIf}
    SetErrorLevel 0
    Quit
  ${EndIf}
  Call ValidateCurrentOwnership
  Call PrepareOwnership
  !insertmacro PreflightInstall
  !insertmacro PrepareApplication
  ${If} $ExistingKey != ""
    ${If} $ExistingKind != "native"
      !insertmacro Progress 20 "Removing the previous installation..."
      !insertmacro RemovePrevious
    ${ElseIf} $OwnershipSupported == 1
      !insertmacro Progress 20 "Preparing the existing installation..."
      !insertmacro BeforeReplace
      Call RemoveObsoleteFiles
    ${Else}
      !insertmacro Progress 20 "Removing the previous installation..."
      Call RemoveInstalledVersion
    ${EndIf}
  ${EndIf}
  !insertmacro Progress 40 "Installing application files..."
  ; Try mode returns extraction errors to our same-window UI instead of opening
  ; a second NSIS retry/ignore dialog. Every failed write stops the installation.
  SetOverwrite try
  ClearErrors
  SetOutPath "$INSTDIR"
  File "/oname=${APP_EXE}" "${PAYLOAD_EXE}"
  ${If} ${Errors}
    StrCpy $Failure "Unable to install ${APP_NAME}. Close the application and retry Setup."
    Call Fail
  ${EndIf}
  !insertmacro InstallFiles
  ClearErrors
  WriteUninstaller "$INSTDIR\uninstall.exe"
  ${If} ${Errors}
    StrCpy $Failure "Unable to write the uninstaller. Close other Setup windows and retry."
    Call Fail
  ${EndIf}
  SetOutPath "$TEMP"
  !insertmacro Progress 75 "Registering ${APP_NAME}..."
  Call RegisterInstallation
  !insertmacro AfterInstall
  Call CommitOwnership
  ${If} $RestartApp == 1
    Call Launch
  ${EndIf}
  !insertmacro Progress 100 "Installation complete."
  SetErrorLevel 0
SectionEnd
