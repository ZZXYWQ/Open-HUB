!include "${__FILEDIR__}\preserve-backups.nsh"
!macro NSIS_HOOK_PREINSTALL
  !insertmacro OPEN_HUB_PRESERVE_BACKUPS "$LOCALAPPDATA\Open GHub MCHOSE\backups" "$LOCALAPPDATA\Open HUB\backups"
  ; Gracefully close the current product before an upgrade.
  ${If} ${FileExists} "$INSTDIR\${MAINBINARYNAME}.exe"
    nsExec::ExecToLog /TIMEOUT=10000 '"$INSTDIR\${MAINBINARYNAME}.exe" --prepare-update'
    Pop $0
    Sleep 1200
  ${EndIf}
  ; Migrate the previous local fork. Keep its application data identifier.
  ReadRegStr $R7 HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Open GHub MCHOSE" "InstallLocation"
  ${If} $R7 != ""
    nsis_tauri_utils::StrReplace "$R7" '$\"' ""
    Pop $R7
    ${If} ${FileExists} "$R7\open-ghub.exe"
    ${AndIf} ${FileExists} "$R7\uninstall.exe"
      nsExec::ExecToLog /TIMEOUT=10000 '"$R7\open-ghub.exe" --prepare-update'
      Pop $0
      Sleep 1200
      ExecWait '"$R7\uninstall.exe" /S _?=$R7' $0
      ${If} $0 != 0
        Abort "旧版退出失败，请关闭旧版后重试安装 Open HUB。"
      ${EndIf}
      Delete "$R7\uninstall.exe"
      RMDir "$R7"
    ${EndIf}
  ${EndIf}

!macroend
!macro NSIS_HOOK_PREUNINSTALL
  ${If} ${FileExists} "$INSTDIR\${MAINBINARYNAME}.exe"
    nsExec::ExecToLog /TIMEOUT=10000 '"$INSTDIR\${MAINBINARYNAME}.exe" --prepare-update'
    Pop $0
    Sleep 1200
  ${EndIf}
!macroend
