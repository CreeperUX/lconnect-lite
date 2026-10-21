# 冷头屏幕（LCD-S 480×480）：移植方案

来源：lian-li-linux（`crates/lianli-devices/src/winusb/lcd/`、`crypto.rs`、`lianli-media`，MIT）。

## 推一帧 JPEG

- 屏幕参数：480×480、最高 60 fps、JPEG 质量 85、单帧 ≤ 153 600 字节；设备不会旋转，旋转在编码前用软件做。
- 初始化：StopPlay 0x7B（每次会话一次）→ GetVer 0x0A → FrameRate 0x0F `[30]` → SyncClock 0x33（mode 2）→ StopClock 0x34 `[0]`，每条读一个 512 字节回复。
- 帧：DES 头（cmd 0x65，参数 = JPEG 长度 u32 BE）+ JPEG，**一次写完**；读一个回复，`[8]` 是缓冲水位，>3 时每 50 ms 发 QueryBlock 0x7A，直到 ≤2。
- **写超时必须 2 s**：超时太短会把传输截断在中途，固件失步，只能断电恢复。（我们 `h2.rs` 里 200 ms 的写超时对 512 字节命令没问题，推图必须用 2 s。）
- JPEG 帧不算“流式播放”，0xFA/0xFB 控制命令可以穿插，只要写和读在同一把锁里完成（我们按命令号匹配回复，已经满足）。只有 H.264 播放期间要暂缓控制命令。

## 命令

| 命令 | 参数 | 说明 |
|---|---|---|
| 亮度 0x0E | `[0..100]` | 回复读掉即可 |
| 旋转 0x0D | `[r & 3]` | lian-li-linux 不用，改为软件旋转 |
| 帧率 0x0F | `[fps]` | |
| 时钟 0x33 | `[年高, 年低, 月, 日, 时, 分, 秒, mode]` | 只用过 mode 2，随后发 StopClock 0x34 `[0]`；离线时钟的 mode 字节要再反编译 lianli.lcd207.dll 确认 |
| StartPlay 0x79 | `[块长 u32, 是否最后 u8, 播放次数 u8, tick u32]` | H.264 分块，每块 202 752 字节 |

## 不建议做的

- **开机图 0x28**：会写闪存，传输中断后存储状态未知；lian-li-linux 对 H2 的 USB 路径也没启用。跳过。
- **桌面模式 0x96**：冷头会重新枚举成 CH340 副显示器（方屏 1A86:AD22），期间泵和风扇控制失效；切回需要向 HID 接口写 `"5f3759df"`。最多做一个“切回 LCD 模式”的救援功能。

## 媒体管线（Windows）

- 纯 Rust 即可：`image`（JPEG 编码、PNG/GIF 解码）、`ab_glyph`（字体）。turbojpeg 需要 cmake + nasm，不用；480×480 用 `image` 编码只要几毫秒。
- GIF 可以拆成 JPEG 帧流播放，不需要 H.264。视频可选：调用用户提供的 `ffmpeg.exe` 转成 JPEG 帧。

## 传感器仪表盘

- 480×480 画布：背景色/图 + 弧形仪表（逐像素画）+ 文字（ab_glyph + 内嵌 OFL 字体），显示水温、泵速、CPU/GPU；数值不变就不重推，默认 1 fps。

## 顺序

1. 初始化 + 亮度（约 40 行）
2. 静态 JPEG（约 100 行，加 `image`）
3. 软件旋转（约 20 行）
4. 传感器仪表盘（约 300 行，加 `ab_glyph`）
5. GIF 转 JPEG 帧流（约 150 行）
6. 可选：视频（ffmpeg.exe → JPEG 帧）
7. 离线时钟（确认 mode 字节后约 30 行）
