; Registry manifest v4. File paths are relative to the registered InstallLocation.
; The same enumeration supplies registry publication and normal uninstall.
!define OWNERSHIP_PROTOCOL 4
!macro OwnedFiles CALLBACK
  !insertmacro ${CALLBACK} "${APP_EXE}"
  !insertmacro ${CALLBACK} "uninstall.exe"
  !insertmacro AppOwnedFiles "${CALLBACK}"
!macroend

; Only canonical relative file paths are accepted. Check each existing component
; for reparse points, so a directory junction cannot redirect deletion elsewhere.
Function ValidateOwnedPath
  StrLen $0 $OwnedPath
  ${If} $0 == 0
  ${OrIf} $0 > 900
    Goto invalid_owned_path
  ${EndIf}
  ; The Win32 lexical check also accepts already-missing obsolete files.
  System::Call 'kernel32::GetFullPathNameW(w "$INSTDIR\$OwnedPath", i ${NSIS_MAX_STRLEN}, w .r1, p 0) i.r0'
  ${If} $0 == 0
  ${OrIf} $0 >= ${NSIS_MAX_STRLEN}
  ${OrIf} $1 != "$INSTDIR\$OwnedPath"
    Goto invalid_owned_path
  ${EndIf}
  StrCpy $OwnedWalk "$INSTDIR"
  StrCpy $OwnedComponent ""
  StrCpy $OwnedIndex 0
  System::Call 'kernel32::GetFileAttributesW(w "$OwnedWalk") i.r1'
  IntOp $2 $1 & 0x400
  ${If} $1 != -1
  ${AndIf} $2 != 0
    Goto invalid_owned_path
  ${EndIf}
  ${Do}
    StrCpy $2 $OwnedPath 1 $OwnedIndex
    ${If} $2 == "\"
    ${OrIf} $2 == ""
      ${If} $OwnedComponent == ""
      ${OrIf} $OwnedComponent == "."
      ${OrIf} $OwnedComponent == ".."
        Goto invalid_owned_path
      ${EndIf}
      StrCpy $3 $OwnedComponent 1 -1
      ${If} $3 == "."
      ${OrIf} $3 == " "
        Goto invalid_owned_path
      ${EndIf}
      StrCpy $OwnedWalk "$OwnedWalk\$OwnedComponent"
      System::Call 'kernel32::GetFileAttributesW(w "$OwnedWalk") i.r1'
      IntOp $3 $1 & 0x400
      ${If} $1 != -1
      ${AndIf} $3 != 0
        Goto invalid_owned_path
      ${EndIf}
      ${If} $2 == ""
        ${ExitDo}
      ${EndIf}
      StrCpy $OwnedComponent ""
    ${Else}
      ${If} $2 == ":"
      ${OrIf} $2 == "/"
      ${OrIf} $2 == "*"
      ${OrIf} $2 == "?"
      ${OrIf} $2 == '$\"'
      ${OrIf} $2 == "<"
      ${OrIf} $2 == ">"
      ${OrIf} $2 == "|"
      ${OrIf} $2 == "$\r"
      ${OrIf} $2 == "$\n"
      ${OrIf} $2 == "$\t"
        Goto invalid_owned_path
      ${EndIf}
      StrCpy $OwnedComponent "$OwnedComponent$2"
    ${EndIf}
    IntOp $OwnedIndex $OwnedIndex + 1
  ${Loop}
  Return
invalid_owned_path:
  StrCpy $Failure "The installed file list contains an unsafe path: $OwnedPath"
  Call Fail
FunctionEnd

Function OpenOwnedList
  ClearErrors
  FileOpen $OwnedHandle "$PLUGINSDIR\previous-owned-files.txt" r
  ${If} ${Errors}
    Goto invalid_owned_list
  ${EndIf}
  FileSeek $OwnedHandle 0 END $0
  IntOp $0 $0 & 1
  ${If} $0 != 0
    Goto invalid_owned_list
  ${EndIf}
  FileSeek $OwnedHandle 0 SET
  FileReadUTF16LE $OwnedHandle $OwnedLine
  ${If} $OwnedLine != "NativeNSIS-Ownership-${OWNERSHIP_PROTOCOL}$\r$\n"
    Goto invalid_owned_list
  ${EndIf}
  FileReadUTF16LE $OwnedHandle $OwnedLine
  ${If} $OwnedLine != "[Files]$\r$\n"
    Goto invalid_owned_list
  ${EndIf}
  StrCpy $OwnedCount 0
  StrCpy $OwnedHasApp 0
  StrCpy $OwnedHasUninstaller 0
  Return
invalid_owned_list:
  StrCpy $Failure "The installed file ownership list is invalid."
  Call Fail
FunctionEnd

Function ReadOwnedPath
  ClearErrors
  FileReadUTF16LE $OwnedHandle $OwnedLine
  ${If} ${Errors}
    StrCpy $Failure "The installed file ownership list is truncated."
    Call Fail
  ${EndIf}
  ${If} $OwnedLine == "[RegistryKeys]$\r$\n"
    ; Registry ownership is consumed separately from file paths. This toolkit
    ; owns its registration key; RegisterInstallation refreshes it on upgrade.
    FileReadUTF16LE $OwnedHandle $OwnedLine
    ${If} $OwnedLine != "HKLM:64\${APP_KEY}$\r$\n"
      StrCpy $Failure "The installed registry ownership section is invalid."
      Call Fail
    ${EndIf}
    FileReadUTF16LE $OwnedHandle $OwnedLine
    ${If} $OwnedLine != ":END$\r$\n"
      StrCpy $Failure "The installed ownership list is truncated or invalid."
      Call Fail
    ${EndIf}
    FileReadUTF16LE $OwnedHandle $0
    ${IfNot} ${Errors}
      StrCpy $Failure "The installed file ownership list has trailing data."
      Call Fail
    ${EndIf}
    StrCpy $OwnedPath ""
    Return
  ${EndIf}
  IntOp $OwnedCount $OwnedCount + 1
  StrCpy $0 $OwnedLine 2 -2
  ${If} $0 != "$\r$\n"
  ${OrIf} $OwnedCount > 100000
    StrCpy $Failure "The installed file ownership list is truncated or too large."
    Call Fail
  ${EndIf}
  ${TrimNewLines} $OwnedLine $OwnedPath
  Call ValidateOwnedPath
  ${If} $OwnedPath == $OwnedApp
    StrCpy $OwnedHasApp 1
  ${EndIf}
  ${If} $OwnedPath == "uninstall.exe"
    StrCpy $OwnedHasUninstaller 1
  ${EndIf}
FunctionEnd

!macro ValidateOwnedFile PATH
  StrCpy $OwnedPath "${PATH}"
  Call ValidateOwnedPath
!macroend

Function ValidateCurrentOwnership
  !insertmacro OwnedFiles ValidateOwnedFile
FunctionEnd

!macro PushOwnedFile PATH
  Push "${PATH}"
  IntOp $OwnedCount $OwnedCount + 1
!macroend

; Publish only after payload extraction and integrations have succeeded. The
; protocol marker is the commit flag: partial publication is never advertised.
Function CommitOwnership
  DeleteRegValue HKLM "${APP_KEY}" "FileOwnershipProtocol"
  StrCpy $OwnedCount 0
  !insertmacro OwnedFiles PushOwnedFile
  native_nsis_ui::WriteRegistryList /NOUNLOAD "HKLM:64" "${APP_KEY}" "OwnedFiles" "$OwnedCount"
  Pop $Failure
  !insertmacro CheckResult
  Push "HKLM:64\${APP_KEY}"
  native_nsis_ui::WriteRegistryList /NOUNLOAD "HKLM:64" "${APP_KEY}" "OwnedRegistryKeys" "1"
  Pop $Failure
  !insertmacro CheckResult
  ClearErrors
  WriteRegDWORD HKLM "${APP_KEY}" "FileOwnershipProtocol" ${OWNERSHIP_PROTOCOL}
  ${If} ${Errors}
    StrCpy $Failure "Unable to publish the file ownership protocol."
    Call Fail
  ${EndIf}
FunctionEnd

Function PrepareOwnership
  StrCpy $OwnershipSupported 0
  ${If} $ExistingKind != "native"
    Return
  ${EndIf}
  ClearErrors
  ReadRegDWORD $0 HKLM "${APP_KEY}" "FileOwnershipProtocol"
  ${If} ${Errors}
  ${OrIf} $0 != ${OWNERSHIP_PROTOCOL}
    Return
  ${EndIf}
  ; Native registration records the installed executable as an exact DisplayIcon
  ; path. Validate that identity, rather than requiring the incoming exe name.
  ReadRegStr $OwnedApp HKLM "${APP_KEY}" "DisplayIcon"
  StrLen $0 "$INSTDIR\"
  StrCpy $1 $OwnedApp $0
  ${If} $1 != "$INSTDIR\"
    StrCpy $Failure "The installed application executable registration is invalid."
    Call Fail
  ${EndIf}
  StrCpy $OwnedApp $OwnedApp "" $0
  StrCpy $OwnedPath $OwnedApp
  Call ValidateOwnedPath
  InitPluginsDir
  ClearErrors
  FileOpen $OwnedHandle "$PLUGINSDIR\previous-owned-files.txt" w
  ${If} ${Errors}
    StrCpy $Failure "Unable to prepare the installed ownership snapshot."
    Call Fail
  ${EndIf}
  FileWriteUTF16LE $OwnedHandle "NativeNSIS-Ownership-${OWNERSHIP_PROTOCOL}$\r$\n[Files]$\r$\n"
  native_nsis_ui::ReadRegistryList /NOUNLOAD "HKLM:64" "${APP_KEY}" "OwnedFiles"
  Pop $Failure
  Pop $OwnedRemaining
  !insertmacro CheckResult
  ${DoWhile} $OwnedRemaining > 0
    Pop $OwnedPath
    IntOp $OwnedRemaining $OwnedRemaining - 1
    ; Validate each registry entry before framing it: embedded newlines cannot
    ; impersonate a section boundary in the private snapshot.
    Call ValidateOwnedPath
    FileWriteUTF16LE $OwnedHandle "$OwnedPath$\r$\n"
  ${Loop}
  FileWriteUTF16LE $OwnedHandle "[RegistryKeys]$\r$\n"
  native_nsis_ui::ReadRegistryList /NOUNLOAD "HKLM:64" "${APP_KEY}" "OwnedRegistryKeys"
  Pop $Failure
  Pop $OwnedRemaining
  !insertmacro CheckResult
  ${If} $OwnedRemaining != 1
    StrCpy $Failure "The installed registry ownership section is invalid."
    Call Fail
  ${EndIf}
  Pop $OwnedLine
  ${If} $OwnedLine != "HKLM:64\${APP_KEY}"
    StrCpy $Failure "The installed registry ownership section is invalid."
    Call Fail
  ${EndIf}
  FileWriteUTF16LE $OwnedHandle "$OwnedLine$\r$\n:END$\r$\n"
  FileClose $OwnedHandle
  ${If} ${Errors}
    StrCpy $Failure "Unable to save the installed ownership snapshot."
    Call Fail
  ${EndIf}
  ; Validate the entire snapshot before preflight, integrations or files change.
  Call OpenOwnedList
  ${Do}
    Call ReadOwnedPath
    ${If} $OwnedPath == ""
      ${ExitDo}
    ${EndIf}
  ${Loop}
  FileClose $OwnedHandle
  ${If} $OwnedCount < 2
  ${OrIf} $OwnedHasApp != 1
  ${OrIf} $OwnedHasUninstaller != 1
    StrCpy $Failure "The installed file ownership list is incomplete."
    Call Fail
  ${EndIf}
  StrCpy $OwnershipSupported 1
  ClearErrors
FunctionEnd

!macro MatchOwnedFile PATH
  ${If} $OwnedPath == "${PATH}"
    StrCpy $OwnedKeep 1
  ${EndIf}
!macroend

Function RemoveObsoleteFiles
  Call OpenOwnedList
  ${Do}
    Call ReadOwnedPath
    ${If} $OwnedPath == ""
      ${ExitDo}
    ${EndIf}
    StrCpy $OwnedKeep 0
    !insertmacro OwnedFiles MatchOwnedFile
    ${If} $OwnedKeep == 0
      ClearErrors
      Delete "$INSTDIR\$OwnedPath"
      ${If} ${Errors}
        FileClose $OwnedHandle
        StrCpy $Failure "An obsolete application file could not be removed. Close the application and retry Setup."
        Call Fail
      ${EndIf}
      ; Remove only empty parents of the obsolete file, stopping at INSTDIR.
      ${GetParent} "$INSTDIR\$OwnedPath" $OwnedWalk
      ${DoWhile} $OwnedWalk != $INSTDIR
        ClearErrors
        RMDir "$OwnedWalk"
        ${If} ${Errors}
          ${ExitDo}
        ${EndIf}
        ${GetParent} "$OwnedWalk" $OwnedWalk
      ${Loop}
    ${EndIf}
  ${Loop}
  FileClose $OwnedHandle
FunctionEnd

!macro DeleteOwnedFile PATH
  !if "${PATH}" != "uninstall.exe"
    Delete "$INSTDIR\${PATH}"
  !endif
!macroend
