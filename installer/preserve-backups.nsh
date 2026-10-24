!ifndef OPEN_HUB_PRESERVE_BACKUPS
!define OPEN_HUB_PRESERVE_BACKUPS
!include "LogicLib.nsh"

; Preserve before uninstall. Atomically reserve a fresh directory, so neither
; an existing backup nor a previous migration snapshot can be overwritten.
!macro OPEN_HUB_PRESERVE_BACKUPS SOURCE DEST
  ${If} ${FileExists} "${SOURCE}\*.*"
    Push $R8
    Push $R9
    Push $9
    ClearErrors
    CreateDirectory "${DEST}"
    ${If} ${Errors}
      SetErrorLevel 2
      Abort "无法创建备份目录；已取消迁移，旧版文件保留。"
    ${EndIf}
    StrCpy $R9 0
    ${Do}
      StrCpy $R8 "${DEST}\legacy-mchose-$R9"
      System::Call 'kernel32::CreateDirectoryW(w "$R8", p 0) i.r9'
      ${If} $9 != 0
        ${ExitDo}
      ${EndIf}
      IntOp $R9 $R9 + 1
      ${If} $R9 >= 1000
        SetErrorLevel 2
        Abort "无法创建独立备份目录；已取消迁移，旧版文件保留。"
      ${EndIf}
    ${Loop}
    ClearErrors
    CopyFiles /SILENT "${SOURCE}\*.*" "$R8"
    ${If} ${Errors}
      SetErrorLevel 2
      Abort "旧版备份复制失败；已取消迁移，原始文件保留。"
    ${EndIf}
    Pop $9
    Pop $R9
    Pop $R8
  ${EndIf}
!macroend
!endif
