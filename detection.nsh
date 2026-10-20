; Registration discovery and unelevated/elevated routing. No payload extraction.
Function ReadInstallation
  StrCpy $ExistingKey ""
  StrCpy $ExistingKind ""
  StrCpy $OldUninstaller ""
  ClearErrors
  ReadRegStr $0 HKLM "${APP_KEY}" "InstallLocation"
  ${IfNot} ${Errors}
    StrCpy $ExistingKey "${APP_KEY}"
    StrCpy $INSTDIR $0
    StrCpy $ExistingKind "legacy"
    ReadRegStr $1 HKLM "${APP_KEY}" "${APP_MARKER}"
    ${If} $1 == "${APP_MARKER_VALUE}"
      StrCpy $ExistingKind "native"
    ${EndIf}
  ${EndIf}
  !insertmacro DiscoverPrevious
  ${If} $ExistingKey != ""
    ReadRegStr $OldUninstaller HKLM "$ExistingKey" "UninstallString"
    ReadRegStr $ExistingVersion HKLM "$ExistingKey" "DisplayVersion"
  ${EndIf}
FunctionEnd

Function InitializeSetup
  SetRegView 64
  Call ConfigureUI
  SetShellVarContext all
  ${IfNot} ${RunningX64}
  ${OrIfNot} ${AtLeastWin10}
    StrCpy $Failure "${APP_NAME} requires 64-bit Windows 10 or later."
    Call Fail
  ${EndIf}
  Call ReadOptions
  ${If} $Action == ""
    StrCpy $Action "install"
  ${EndIf}
  ${If} $Worker == 1
    SetSilent silent
    native_nsis_ui::IsElevated /NOUNLOAD
    Pop $0
    ${If} $0 != 1
      StrCpy $Failure "The installation worker requires Windows administrator permission."
      Call Fail
    ${EndIf}

  ${ElseIf} $Automatic == 1
    SetSilent silent
  ${EndIf}
  Call ReadInstallation
  Call CheckInstallLocation
  ${If} $ExistingKey != ""
    ${If} $ExistingKind == "native"
      IfFileExists "$INSTDIR\uninstall.exe" +3
        StrCpy $Failure "The previous uninstaller is missing. Restore it before reinstalling or removing this installation."
        Call Fail
    ${Else}
      IfFileExists "$INSTDIR\${APP_EXE}" +3
        StrCpy $Failure "The registered ${APP_NAME} executable is missing. Repair the previous installation first."
        Call Fail
      ${If} $OldUninstaller == ""
        StrCpy $Failure "The registered previous uninstaller is missing."
        Call Fail
      ${EndIf}
    ${EndIf}
    ${VersionCompare} "$ExistingVersion" "${VERSION}" $0
    ${If} $0 == 1
    ${AndIf} $Automatic == 1
      StrCpy $Failure "A newer ${APP_NAME} version ($ExistingVersion) is installed. Automatic updates cannot downgrade. Run Setup interactively and choose Downgrade."
      Call Fail
    ${EndIf}
    ${If} $Automatic != 1
    ${AndIf} $Worker != 1
      IfSilent 0 +3
        StrCpy $Failure "${APP_NAME} is already installed. Run Setup interactively to choose an action."
        Call Fail

      ${If} $0 == 2
        StrCpy $1 "upgrade"
      ${ElseIf} $0 == 1
        StrCpy $1 "downgrade"
      ${Else}
        StrCpy $1 "reinstall"
      ${EndIf}
      native_nsis_ui::ConfigureMaintenance /NOUNLOAD "$1" "$ExistingVersion" "${VERSION}"
      Pop $Failure
      !insertmacro CheckResult
      native_nsis_ui::Run /NOUNLOAD "maintenance" "$INSTDIR" "$RestartApp" "$WorkerArguments"
      Pop $0
      SetErrorLevel $0
      Quit
    ${EndIf}
  ${ElseIf} $Automatic == 1
    StrCpy $Failure "Update requires an existing registered installation."
    Call Fail
  ${Else}
    ${If} $Action != "install"
      StrCpy $Failure "Reinstall and uninstall require an existing registered installation."
      Call Fail
    ${EndIf}
    ${If} $Worker != 1
      IfSilent selection_done
      native_nsis_ui::Run /NOUNLOAD "install" "$INSTDIR" "$RestartApp" "$WorkerArguments"
      Pop $0
      SetErrorLevel $0
      Quit
    ${EndIf}
  ${EndIf}
selection_done:
  Call CheckInstallLocation
  ${If} $Worker != 1
    StrCpy $0 "$WorkerArguments"
    ${If} $Automatic == 1
      StrCpy $0 "$0 /AUTOMATIC"
    ${EndIf}
    ${If} $RestartApp == 1
      StrCpy $0 "$0 /RESTARTAPP"
    ${EndIf}
    native_nsis_ui::Execute /NOUNLOAD "$EXEPATH" "/S /WORKER /ACTION=$Action $0 /D=$INSTDIR"
    Pop $Failure
    Pop $0
    !insertmacro CheckResult
    SetErrorLevel $0
    Quit
  ${EndIf}
FunctionEnd
