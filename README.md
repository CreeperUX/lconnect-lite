# LConnect Lite

联力 L-Connect 3 的轻量替代：控制隐流二代（HydroShift II LCD）冷头和 L-Wireless 无线风扇，
覆盖散热曲线、灯效、冷头屏幕、无线绑定。常驻的只有后台服务（托盘图标，约 3–23 MB）；
图形界面需要时才打开，关掉后不占资源。功能对照见 [docs/features.md](docs/features.md)。

## 结构

| 目录 | 内容 |
|---|---|
| `crates/lcl-devices` | USB 协议层：冷头（泵、读数、灯圈、屏幕）、无线收发器（PWM、灯效上传、绑定）、tinyuz FFI |
| `crates/lcl-effects` | 灯效渲染：TL 风扇 28 种、冷头灯圈 11 种（移植自 lian-li-linux） |
| `crates/lcl-media` | 屏幕画面：图片/GIF 转 480×480 JPEG、传感器仪表盘 |
| `crates/lcl-core` | 后台服务和界面共用：配置、状态、命名管道协议、预设、GPU 读数 |
| `crates/lcl-sensord` | 传感器系统服务：以 SYSTEM 身份经 PawnIO 读 CPU 温度和功耗，命名管道 `lconnect-lite-sensors` 交给后台服务；`--console` 调试 |
| `crates/lcl-daemon` | 后台服务：控制循环、屏幕线程、托盘、开机自启、日志；另带 `probe` / `run` 调试命令 |
| `app` | Tauri 2 界面（Vite + TypeScript），只通过命名管道和后台服务通信 |
| `vendor` | tinyuz 与 HDiffPatch 的 C++ 源码（MIT），灯效数据压缩用 |
| `docs` | 协议文档、功能对照、移植方案、硬件实测记录 |

```
界面 (Tauri) ──命名管道 \\.\pipe\lconnect-lite──▶ lcl-daemon ──WinUSB──▶ 冷头 / 无线收发器
                                                   ├ 控制线程：泵、风扇、灯效、绑定
                                                   ├ 屏幕线程：仪表盘 / 图片 / GIF
                                                   └ 主线程：托盘
```

## 安装

运行 `LConnect Lite_<版本>_x64-setup.exe`（需要管理员权限），装到 `C:\Program Files\LConnect Lite`，和官方软件一样出现在“应用”列表里，可以从那里卸载。安装程序会：

- 检测到 L-Connect 3 的服务时，询问是否停止并禁用它（WinUSB 同一时间只允许一个程序打开设备）；卸载本软件时可以选择恢复
- 没装过 PawnIO 驱动时一起装上（读 CPU 温度和功耗要用），并注册传感器系统服务 `LConnectLiteSensors`；卸载时只删服务，不卸驱动
- 开机自启用计划任务（登录时立即启动，没有开机项那约 10 秒的延迟），启动后台服务（托盘图标），加开始菜单快捷方式
- 开机到登录、后台服务还没启动的这段时间，传感器服务按你的散热设置先接管风扇和泵（设置副本在 `%ProgramData%\LConnect Lite\control.toml`），后台服务一启动就交给它；过程记在同目录的 `keeper.log`
- 第一次运行只按默认曲线控制风扇和泵，灯效和屏幕保持设备原样，需要时在界面里打开

设备用的是 Windows 自带的 WinUSB 驱动，不需要另装驱动，也不依赖官方软件。

- 配置：`%APPDATA%\LConnect Lite\config.toml`
- 日志：`%LOCALAPPDATA%\LConnect Lite\daemon.log`
- 卸载时勾选“删除应用程序数据”会一并删除这两个目录

## 打包

```powershell
cd app; npm install; cd ..
.\scripts\build-installer.ps1
```

生成 `target\release\bundle\nsis\LConnect Lite_<版本>_x64-setup.exe`。安装程序是 Tauri 的 NSIS 模板，
文案、图片和安装 / 卸载时的额外步骤在 `app/src-tauri/installer/`（`hooks.nsh` 要保存为带 BOM 的 UTF-8）。
安装包没有代码签名，别的电脑上运行时 SmartScreen 会提示“未知发布者”，点“更多信息 → 仍要运行”。

开发时也可以不打包，直接从编译目录运行（先退出已安装的后台服务）：

```powershell
cargo build --release -p lcl-daemon
cd app; npx tauri build --no-bundle; cd ..
.\target\release\lcl-daemon.exe
.\target\release\lcl-app.exe
```

界面和后台服务放在同一目录；界面打开时后台服务没在运行会自动启动它，后台服务同一时间只会有一个。

## 发布

安装包挂在 GitHub Releases（https://github.com/CreeperUX/lconnect-lite/releases）。发新版本：

1. 改根目录 `Cargo.toml` 的 `version` 和 `app/src-tauri/tauri.conf.json` 的 `version`（两者要一致），提交并推送
2. 运行 `.\scripts\release.ps1`（加 `-Draft` 先建草稿）：打安装包、打 `v<版本>` 标签、建 Release 并上传安装包

## 开发

- 不碰设备、只开命名管道：`lcl-daemon serve --no-devices`
- 只调界面：`cd app && npm run dev`，浏览器打开 http://localhost:1420 ，不在 Tauri 里运行时自动用模拟数据（`app/src/mock.ts`）
- 类型检查：`npm run check`；测试：`cargo test --workspace`
- 调试命令（后台服务没在运行时）：`lcl-daemon probe`、`lcl-daemon run --pump-rpm 2400 --fan 50 --secs 20`
- 屏幕协议诊断：`cargo run -p lcl-devices --example lcd_probe -- frame.jpg`

## 许可

MIT。协议逻辑、灯效渲染、tinyuz 封装移植自 lian-li-linux；tinyuz、HDiffPatch 原样收录，
均为 MIT，见 [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md)。与联力官方无关。
