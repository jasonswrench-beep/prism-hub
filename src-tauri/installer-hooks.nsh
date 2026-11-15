; Added to the hub's installer (tauri.conf.json, bundle.windows.nsis.installerHooks).
;
; Uninstalling the hub removes the hub, its start-up entry, its cached catalog and its download folder (all under
; %LOCALAPPDATA%\<the hub's identifier>). It never touches the suite's apps: they stay in Windows' installed apps list
; and keep working.

!macro NSIS_HOOK_POSTUNINSTALL
  ; An update runs the new setup over the old copy; only a real uninstall clears the hub's own files.
  ${If} $UpdateMode <> 1
    DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "${PRODUCTNAME}"
    RMDir /r "$LOCALAPPDATA\${BUNDLEID}"
  ${EndIf}
!macroend
