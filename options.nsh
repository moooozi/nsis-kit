; Generic installation routing. Consumers interpret their own options via hooks.
Function ReadOptions
  StrCpy $Automatic 0
  StrCpy $RestartApp 0
  StrCpy $Worker 0
  StrCpy $Action ""
  StrCpy $UIWindow 0
  StrCpy $UIActivate 0
  StrCpy $StopOptions 0
  StrCpy $WorkerArguments ""
  !insertmacro InitializeOptions
  ${GetParameters} $0
  native_nsis_ui::Tokenize /NOUNLOAD "$0"
  Pop $Failure
  Pop $OptionCount
  !insertmacro CheckResult
  ${DoWhile} $OptionCount > 0
    Pop $Option
    IntOp $OptionCount $OptionCount - 1
    ${If} $StopOptions == 0
      StrCpy $0 $Option 3
      ${If} $0 == "/D="
        StrCpy $StopOptions 1
      ${Else}
        !insertmacro ParseOption
        ${If} $StopOptions == 0
          ${Switch} $Option
            ${Case} "/AUTOMATIC"
              StrCpy $Automatic 1
              ${Break}
            ${Case} "/RESTARTAPP"
              StrCpy $RestartApp 1
              ${Break}
            ${Case} "/WORKER"
              StrCpy $Worker 1
              ${Break}
            ${Case} "/ACTIVATEUI"
              StrCpy $UIActivate 1
              ${Break}
            ${Default}
              StrCpy $0 $Option 8
              ${If} $0 == "/ACTION="
                StrCpy $Action $Option "" 8
                ${If} $Action != "install"
                ${AndIf} $Action != "reinstall"
                ${AndIf} $Action != "uninstall"
                  StrCpy $Failure "Invalid worker action."
                ${EndIf}
              ${ElseIf} $0 == "/UIHWND="
                StrCpy $UIWindow $Option "" 8
              ${EndIf}
          ${EndSwitch}
        ${EndIf}
      ${EndIf}
    ${EndIf}
  ${Loop}
  !insertmacro CheckResult
  IntFmt $0 "%u" $UIWindow
  ${If} $UIWindow != $0
    StrCpy $Failure "Invalid presentation window."
    Call Fail
  ${EndIf}
  ${If} $Worker != 1
    ${If} $Action != ""
    ${OrIf} $UIWindow != 0
    ${OrIf} $UIActivate != 0
      StrCpy $Failure "Worker options require /WORKER."
      Call Fail
    ${EndIf}
  ${EndIf}
  !insertmacro ValidateOptions
  native_nsis_ui::NormalizeWorkerArguments /NOUNLOAD "$WorkerArguments" "$INSTDIR"
  Pop $Failure
  Pop $WorkerArguments
  !insertmacro CheckResult
FunctionEnd
