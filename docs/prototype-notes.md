# 原型阶段记录（lconnect-lite）

> 这是项目重组前单文件原型的说明，保留其中的实测记录。原型的 `lconnect-lite status` / `run`
> 现在是 `lcl-daemon probe` / `lcl-daemon run`；下文提到的 `.cargo/config.toml` 已不再需要
> （项目移到了短路径下）。

联力隐流二代（HydroShift II LCD）+ L-Wireless 无线风扇的轻量控制原型。
单个 exe（约 0.7 MB），不需要装驱动：设备在 Windows 上已经由系统自带的 WinUSB 驱动接管。

协议逻辑移植自 [lian-li-linux](https://github.com/sgtaziz/lian-li-linux)（MIT），
见 [THIRD_PARTY_NOTICES.md](../THIRD_PARTY_NOTICES.md)。

## 现在能做什么

- `status`：只读。冷头的泵速、水温、冷排风扇转速、无线 MAC；无线发射器 MAC/频道；
  RX 列出的所有无线设备（类型、归属、转速、当前 PWM）；冷头当前是 USB 还是无线控制。
- `run`：每秒向无线设备推一次 PWM 和时钟广播，冷头 USB 模式下每 3.6 秒发一次泵/风扇设定。
  所有风扇共用一个百分比，泵用目标转速。

还没做：温度曲线、灯效、LCD 屏幕、绑定/解绑、SL-INF 右侧串联的反序。

## 运行前

WinUSB 同一时间只允许一个程序打开设备，所以要先停掉官方服务并退出 L-Connect 3（管理员 PowerShell）：

```powershell
Stop-Service LConnectServiceWatcher, LConnectService
```

恢复官方控制：

```powershell
Start-Service LConnectService, LConnectServiceWatcher
```

## 用法

```powershell
lconnect-lite status
lconnect-lite run --pump-rpm 2400 --fan 50 --dry-run --secs 5   # 只打印无线包，冷头只读
lconnect-lite run --pump-rpm 2400 --fan 50                       # Ctrl+C 退出
```

- `--pump-rpm`：方屏 1600-3200，圆屏 1600-2500，超出会被截断。
- `--fan`：百分比，最低 20。
- 退出后设备保持最后收到的转速（实测：TL 无线风扇在没有任何程序发包的几分钟里一直保持原 PWM）。
- 隐流二代 TL 版的冷排风扇是独立的无线设备（RX 列表里的“TL 无线”），冷头本身风扇数为 0，
  所以 USB 读到的“冷排”转速是 0、0xFB 里的风扇字节不起作用。

## 构建

```powershell
cargo build --release
```

`.cargo/config.toml` 把 target 目录放到了 `%TEMP%\lcl-target`，因为项目路径太长会让链接器超出
Windows 260 字符的路径限制。项目搬到短路径后可以删掉这个文件。

## 已知风险

- **无线模式下会覆盖冷头屏幕设置**：参数块里除泵速外还带着主题、亮度、文字颜色，
  原型一律写默认值（主题 0、亮度 80、白色）。
- **冷头读取可能会停掉 LCD 动画**：第一次读不到数据时会发 StopPlay/StopClock 唤醒控制通道。
- 冷头 0xFB 里三个风扇字节能否分别控制还没验证（TL 版用不到）。

## 实测记录（方屏 LCD-S + 3×TL 无线，2026-10-21）

- 风扇 50%：约 1660 → 1260 rpm，2 秒内到位，停发后保持。
- 泵 2400 rpm：实际稳定在约 2470–2500 rpm（映射表本身偏高约 80 rpm），停发后至少 8 秒保持不变。
- 每次启动 `run` 的前约 10 秒，泵会先掉到约 2076–2300 rpm，大约第 3 次 0xFB 之后才到目标。
  原因未明；单纯的 `status`（只发 0xFA）不会引起这个下降。
- RX 偶尔回不完整的数据块（30 轮里约 2 轮），会跳过这一轮、用缓存的设备列表继续发 PWM。
- 设环境变量 `LCL_DEBUG=1` 可看到被丢弃的冷头回复。
