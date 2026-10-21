; LConnect Lite 安装程序钩子：Tauri 的 NSIS 模板在安装 / 卸载的对应位置插入这几个宏。
; 安装程序以管理员身份运行（perMachine），HKCU 是点了“是”的那个用户。
; 提示文字按安装语言选（2052 = 简体中文），本文件要带 BOM，否则中文会乱码。

; 中文语言包默认用宋体，换成微软雅黑 UI（英文界面用 Segoe UI）。
SetFont /LANG=2052 "Microsoft YaHei UI" 9
SetFont /LANG=1033 "Segoe UI" 9

; 传感器服务：以 SYSTEM 身份经 PawnIO 读 CPU 温度和功耗。
!define LCL_SENSOR_SVC "LConnectLiteSensors"

!macro NSIS_HOOK_PREINSTALL
  ; 旧版本的后台服务和传感器服务还在运行时文件替换不了（界面由模板自己检查）。
  nsExec::Exec 'taskkill /F /IM lcl-daemon.exe'
  Pop $0
  nsExec::Exec 'sc stop ${LCL_SENSOR_SVC}'
  Pop $0
  Sleep 1000

  ; L-Connect 3 的服务会和我们抢设备（WinUSB 同一时间只允许一个程序打开）。
  nsExec::Exec 'sc query LConnectService'
  Pop $0
  ${If} $0 = 0
    ${If} $LANGUAGE = 2052
      StrCpy $1 "检测到 L-Connect 3 的后台服务。$\r$\n$\r$\nLConnect Lite 不能和它同时控制设备。是否停止并禁用 L-Connect 3 服务？$\r$\n$\r$\n卸载 LConnect Lite 时可以选择恢复。选“否”的话，需要你自己先退出 L-Connect 3。"
    ${Else}
      StrCpy $1 "The L-Connect 3 service is installed.$\r$\n$\r$\nLConnect Lite can't control the devices while it runs. Stop and disable the L-Connect 3 service?$\r$\n$\r$\nYou can restore it when uninstalling LConnect Lite."
    ${EndIf}
    MessageBox MB_YESNO|MB_ICONQUESTION "$1" /SD IDYES IDNO lcl_keep_lconnect
      ; 先停看门狗，否则它会把服务重新拉起来。
      nsExec::Exec 'sc config LConnectServiceWatcher start= disabled'
      Pop $0
      nsExec::Exec 'sc config LConnectService start= disabled'
      Pop $0
      nsExec::Exec 'sc stop LConnectServiceWatcher'
      Pop $0
      nsExec::Exec 'sc stop LConnectService'
      Pop $0
      WriteRegDWORD SHCTX "${MANUPRODUCTKEY}" "DisabledLConnect" 1
    lcl_keep_lconnect:
  ${EndIf}
!macroend

!macro NSIS_HOOK_POSTINSTALL
  ; PawnIO 驱动（读 CPU 温度要用）：没装过才装，别的软件（Fan Control 等）装过的不动。
  ; 它的安装信息在 64 位注册表里，安装程序默认读 32 位视图。
  SetRegView 64
  ReadRegStr $0 HKLM "SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\PawnIO" "DisplayVersion"
  SetRegView default
  ${If} $0 == ""
    DetailPrint "安装 PawnIO 驱动……"
    nsExec::Exec '"$INSTDIR\pawnio\PawnIO_setup.exe" -install -silent'
    Pop $0
  ${EndIf}

  ; 开机接管用的配置副本目录：传感器服务（SYSTEM）开机时读，后台服务（普通用户）保存设置时写，
  ; 所以要让 Users 组能改（S-1-5-32-545 = Users，不受系统语言影响）。
  ReadEnvStr $1 ProgramData
  CreateDirectory "$1\LConnect Lite"
  nsExec::Exec 'icacls "$1\LConnect Lite" /grant *S-1-5-32-545:(OI)(CI)M'
  Pop $0

  ; 传感器服务：已有就改路径，没有就新建；开机自动启动，异常退出后 5 秒重启。
  nsExec::Exec 'sc create ${LCL_SENSOR_SVC} binPath= "\"$INSTDIR\lcl-sensord.exe\"" start= auto DisplayName= "LConnect Lite Sensors"'
  Pop $0
  ${If} $0 <> 0
    nsExec::Exec 'sc config ${LCL_SENSOR_SVC} binPath= "\"$INSTDIR\lcl-sensord.exe\"" start= auto'
    Pop $0
  ${EndIf}
  nsExec::Exec 'sc description ${LCL_SENSOR_SVC} "Reads CPU temperature and power for LConnect Lite (via PawnIO)."'
  Pop $0
  nsExec::Exec 'sc failure ${LCL_SENSOR_SVC} reset= 86400 actions= restart/5000/restart/5000/restart/5000'
  Pop $0
  nsExec::Exec 'sc start ${LCL_SENSOR_SVC}'
  Pop $0

  ; 开机自启：以当前用户身份建计划任务（登录时立即启动，没有 Run 项那约 10 秒的延迟），
  ; 顺带删掉旧版本写的 Run 项。由 lcl-daemon 自己建，界面设置页的开关用的是同一套逻辑。
  nsis_tauri_utils::RunAsUser "$INSTDIR\lcl-daemon.exe" "autostart on"
  Pop $0
  ; 以普通用户身份启动后台服务（管理员身份启动的话，界面连不上它的命名管道）。
  ; 完成页勾选“运行”时界面也会尝试启动，后台服务自己保证只有一个在运行。
  nsis_tauri_utils::RunAsUser "$INSTDIR\lcl-daemon.exe" "serve"
  Pop $0
  ; 升级换了图标时让资源管理器刷新图标缓存（SHCNE_ASSOCCHANGED），否则任务栏、开始菜单可能还是旧图标。
  System::Call 'shell32::SHChangeNotify(i 0x08000000, i 0, p 0, p 0)'
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  nsExec::Exec 'taskkill /F /IM lcl-daemon.exe'
  Pop $0
  ; 升级时安装程序会重新创建服务；卸载时删掉。PawnIO 驱动可能还有别的软件在用，不卸。
  nsExec::Exec 'sc stop ${LCL_SENSOR_SVC}'
  Pop $0
  Sleep 1000
  ${If} $UpdateMode <> 1
    nsExec::Exec 'sc delete ${LCL_SENSOR_SVC}'
    Pop $0
    ; 开机自启任务（模板只删 Run 项）。
    nsExec::Exec 'schtasks /delete /tn "${PRODUCTNAME}" /f'
    Pop $0
  ${EndIf}
  ; 记下安装时有没有禁用 L-Connect 3；后面模板可能会删掉这个注册表项。
  ReadRegDWORD $R9 SHCTX "${MANUPRODUCTKEY}" "DisabledLConnect"
  DeleteRegValue SHCTX "${MANUPRODUCTKEY}" "DisabledLConnect"
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  ${If} $UpdateMode <> 1
    ; 模板只删 %APPDATA%\<identifier>；我们的配置和日志在“LConnect Lite”目录下。
    ${If} $DeleteAppDataCheckboxState = 1
      SetShellVarContext current
      RMDir /r "$APPDATA\LConnect Lite"
      RMDir /r "$LOCALAPPDATA\LConnect Lite"
    ${EndIf}
    ; 开机接管用的配置副本只是缓存，卸载就删。
    ReadEnvStr $1 ProgramData
    RMDir /r "$1\LConnect Lite"

    ${If} $R9 = 1
      nsExec::Exec 'sc query LConnectService'
      Pop $0
      ${If} $0 = 0
        ${If} $LANGUAGE = 2052
          StrCpy $1 "安装 LConnect Lite 时禁用了 L-Connect 3 的后台服务。是否恢复（设为自动启动并立即启动）？"
        ${Else}
          StrCpy $1 "The L-Connect 3 service was disabled when LConnect Lite was installed. Restore it (automatic start, start now)?"
        ${EndIf}
        MessageBox MB_YESNO|MB_ICONQUESTION "$1" /SD IDNO IDNO lcl_skip_restore
          nsExec::Exec 'sc config LConnectService start= auto'
          Pop $0
          nsExec::Exec 'sc config LConnectServiceWatcher start= auto'
          Pop $0
          nsExec::Exec 'sc start LConnectService'
          Pop $0
          nsExec::Exec 'sc start LConnectServiceWatcher'
          Pop $0
        lcl_skip_restore:
      ${EndIf}
    ${EndIf}
  ${EndIf}
!macroend
