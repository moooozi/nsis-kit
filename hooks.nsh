; Hooks run in the elevated worker unless otherwise documented.
; Hook failures must set $Failure and Call Fail (or ${PREFIX}Fail on uninstall).
!ifmacrondef DiscoverPrevious
!macro DiscoverPrevious
!macroend
!endif
!ifmacrondef RemovePrevious
!macro RemovePrevious
  StrCpy $Failure "No migration handler is configured for this installation."
  Call Fail
!macroend
!endif
; Read-only application checks, after generic ownership validation.
!ifmacrondef PreflightInstall
!macro PreflightInstall
!macroend
!endif
; Request and wait for application shutdown. No installation mutations here.
!ifmacrondef PrepareApplication
!macro PrepareApplication
!macroend
!endif
!ifmacrondef InstallFiles
!macro InstallFiles
!macroend
!endif
; Every additional packaged file must be enumerated through this callback.
!ifmacrondef AppOwnedFiles
!macro AppOwnedFiles CALLBACK
!macroend
!endif
!ifmacrondef BeforeReplace
!macro BeforeReplace
!macroend
!endif
!ifmacrondef AfterInstall
!macro AfterInstall
!macroend
!endif
!ifmacrondef BeforeUninstall
!macro BeforeUninstall PREFIX
!macroend
!endif
!ifmacrondef DeleteFiles
!macro DeleteFiles
!macroend
!endif
!ifmacrondef RemoveDirectories
!macro RemoveDirectories
!macroend
!endif

; Option hooks run in each frontend/worker before discovery. ParseOption may set
; StopOptions=1 to stop interpretation; the runtime still drains the token stack.
!ifmacrondef InitializeOptions
!macro InitializeOptions
!macroend
!endif
!ifmacrondef ParseOption
!macro ParseOption
!macroend
!endif
!ifmacrondef ValidateOptions
!macro ValidateOptions
!macroend
!endif
