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

## 运行

先停掉官方服务（管理员 PowerShell），WinUSB 同一时间只允许一个程序打开设备：

```powershell
Stop-Service LConnectServiceWatcher, LConnectService
```

构建并启动后台服务（release 版没有控制台窗口，在托盘里）：

```powershell
cargo build --release -p lcl-daemon
.\target\release\lcl-daemon.exe
```

界面：

```powershell
cd app
npm install
npx tauri build --no-bundle
..\target\release\lcl-app.exe
```

界面和后台服务放在同一目录时，后台服务没在运行，界面里可以一键启动；设置页可以打开开机自启。

- 配置：`%APPDATA%\LConnect Lite\config.toml`
- 日志：`%LOCALAPPDATA%\LConnect Lite\daemon.log`

恢复官方控制：

```powershell
Start-Service LConnectService, LConnectServiceWatcher
```

## 开发

- 不碰设备、只开命名管道：`lcl-daemon serve --no-devices`
- 只调界面：`cd app && npm run dev`，浏览器打开 http://localhost:1420 ，不在 Tauri 里运行时自动用模拟数据（`app/src/mock.ts`）
- 类型检查：`npm run check`；测试：`cargo test --workspace`
- 调试命令（后台服务没在运行时）：`lcl-daemon probe`、`lcl-daemon run --pump-rpm 2400 --fan 50 --secs 20`
- 屏幕协议诊断：`cargo run -p lcl-devices --example lcd_probe -- frame.jpg`

## 许可

MIT。协议逻辑、灯效渲染、tinyuz 封装移植自 lian-li-linux；tinyuz、HDiffPatch 原样收录，
均为 MIT，见 [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md)。与联力官方无关。
