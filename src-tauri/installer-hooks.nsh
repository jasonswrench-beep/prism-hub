; Generated from brand/brand.json by tools/apply-brand.mjs. Edit brand.json, not this file.
;
; Uninstalling the hub removes the hub, its start-up entry, its cached catalog, its download
; folder and its settings. It never removes the apps it installed: they stay in Windows'
; installed apps list and keep working.

!macro NSIS_HOOK_POSTUNINSTALL
  ${If} $UpdateMode <> 1
    DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "Prism Hub"
    SetShellVarContext current
    RmDir /r "$LOCALAPPDATA\org.prismopenresearch.hub"
  ${EndIf}
!macroend
