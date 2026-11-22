; Custom NSIS installer hooks for SplitRoute (bundle.windows.nsis.installerHooks).
;
; The splitroute-helper-service runs splitroute-helper.exe from the install
; directory and keeps the file locked: without stopping the service, an update
; (or uninstall) fails with "Error opening file for writing". The installer in
; silent mode terminates only the application process, so the service is
; stopped in PREINSTALL and started again in POSTINSTALL.
;
; Errors are ignored (Pop discards the exit code): on a clean install the
; service does not exist yet, `net stop` exits non-zero — that is not an
; install failure. `net stop` waits for the service to stop before returning.
;
; Note: if the install is aborted (Abort button) between PREINSTALL and
; POSTINSTALL, the service stays stopped until the next install or reboot —
; it is an auto-start service.

!macro NSIS_HOOK_PREINSTALL
  DetailPrint "Stopping splitroute-helper-service"
  nsExec::ExecToLog 'net stop splitroute-helper-service'
  Pop $0
!macroend

!macro NSIS_HOOK_POSTINSTALL
  DetailPrint "Starting splitroute-helper-service"
  nsExec::ExecToLog 'net start splitroute-helper-service'
  Pop $0
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  DetailPrint "Stopping splitroute-helper-service"
  nsExec::ExecToLog 'net stop splitroute-helper-service'
  Pop $0
!macroend
