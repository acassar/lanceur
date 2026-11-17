; Crochets de l'installeur NSIS (voir bundle.windows.nsis.installerHooks dans
; tauri.conf.json).
;
; À la désinstallation, on retire l'entrée « lancement au démarrage » que l'app
; a pu créer (réglage launchAtStartup, voir src/autostart.rs) : sinon Windows
; essaierait de lancer un exe supprimé à chaque ouverture de session.
; Pas pendant une mise à jour ($UpdateMode = 1) : l'app la recrée au prochain
; lancement, mais on garderait sinon un éventuel « désactivé » du Gestionnaire
; des tâches.
!macro NSIS_HOOK_POSTUNINSTALL
  ${If} $UpdateMode <> 1
    DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "Lanceur"
    DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run" "Lanceur"
  ${EndIf}
!macroend
