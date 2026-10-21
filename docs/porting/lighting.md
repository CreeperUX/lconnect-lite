# 灯效：移植方案

来源：lian-li-linux（`crates/lianli-devices`、`crates/lianli-media/src/rgb`，MIT）。

## tinyuz 压缩

- lian-li-linux 用 `cc` 编 C/C++：`vendor/tuz_wrapper.cpp`（86 行，MIT）+ tinyuz 的 `compress/tuz_enc*.cpp`、`decompress/tuz_dec.c`（只给测试用）+ HDiffPatch 的 `libdivsufsort/divsufsort.cpp`。
- tinyuz（pin 1d74ffa）、HDiffPatch（pin 3b9dca7）是 git 子模块，均为 MIT（拉取时再核对 LICENSE）。
- 字典固定 4096（`tinyuz.rs:31`）。有 10 个参考向量（`tests/fixtures/*.yuz`），可用来验证编译结果逐字节一致。

## 无线 RGB 上传（RF 0x20）

- 帧：`[0]=0x12 [1]=0x20 [2..8]=设备 [8..14]=主机 [14..18]=effect_index [18]=包序号 [19]=总包数(=ceil(len/220)+1)`。
- 头包（序号 0）：`[20..24]` 压缩后长度 u32 BE；`[25..27]` 帧数 u16 BE；`[27]` LED 数；`[32..34]` 间隔 tick（0.625 ms）u16 BE；`[34]` tick 小数（百分之一）；TL 的次级字段全 0。
- 数据包（序号 ≥1）：`[20..240]` 压缩流的 220 字节切片，末尾补 0。
- 原始数据：按帧 → LED → R,G,B 排列。限制：1..=4096 帧、1..=255 LED、压缩后 ≤ 12288 字节。
- effect_index：FNV-1a 32 位（压缩数据 ‖ tick ‖ 小数 ‖ 帧数 ‖ LED 数 ‖ 次级字段），大端存放并把最高位置 1（防止设备把残留字节当作 rx）。同样的动画重复上传是幂等的。
- 确认：GetDev 记录 `[20..24]` 等于 effect_index，且记录在 3 s 内、主板灯光同步未开启（`[28] & 0x40` 为 0）。
- 发送：头包重复 4 次（间隔 20 ms），数据包各一次；整个上传须在 3 s 内完成。0x20 帧之后切换目标或命令前至少隔 10 ms。
- 固件空闲重置后会回到默认效果，需要每秒检查 effect_index，不一致就重新上传。
- TL：每个风扇 26 颗 LED（0-12 上弧左→右，13-25 下弧左→右），3 个风扇共 78 颗，按风扇顺序拼接。
- 亮度上限：TL 调色板 R+G+B ≤ 600，冷头 ≤ 570（反复乘 0.95）；亮度档 0-4 再乘 `[0,64,128,192,255]/256`。

## 冷头灯圈（USB 0xFC）

- 512 字节 DES 头（cmd 0xFC，参数 `[4..8]` = 载荷长度 u32 BE），紧跟载荷，一次写完。
- 载荷：`tinyuz(帧 × 24 LED × RGB)` + 帧数 u16 BE + 间隔 tick（u8，≤255）+ LED 数 24。静态色 = 1 帧、间隔 100。
- **上传后冷头会静默**：流程为 StopPlay → StopClock → 写入 → **关闭并重新打开 USB 设备** → 每 250 ms 发 GetVer 直到回复有效（3 s 内）。两次上传至少间隔 3 s；内容相同就跳过；期间暂停 0xFB。

## 效果渲染（`lianli-media/src/rgb/`）

- TL：L-Connect 的 28 种全部实现（外加 Rainbow），17 种与官方 C# 输出做过哈希比对。Voice 是固定节奏，不跟随音频。
- 冷头：11 种全部实现。
- 参数：速度 0-4、亮度 0-4（加“关”）、最多 4 种颜色、部分效果有方向（顺/逆时针），TL 还有上弧/下弧/全部分区。
- 规模：TL 约 2420 行（35 个文件），冷头约 450 行；依赖 `lianli_shared::rgb` 的几个类型，可在本地重新声明。

## 主板灯光同步（RF 0x27）

见 [binding.md](binding.md)。上传灯效前要求同步处于关闭状态。

## 移植顺序

1. tinyuz：拉取源码、`build.rs`、FFI、参考向量测试。
2. 无线层：`RfDevice` 加 effect_index / cmd_seq / 状态位；0x20 后 10 ms 间隔。
3. 无线 RGB 上传 + 确认 + 每秒漂移检查。先在 78 颗 TL LED 上测静态色。
4. 冷头 0xFC：停止 → 推送 → 重开设备 → GetVer 轮询；`usb.rs` 需要支持重新打开。
5. 渲染器：TL 引擎/调色板/合成 → 常用效果 → 其余 TL → 冷头。
6. 0x27 关闭主板灯光同步（仅在 `[28] & 0x40` 置位时需要）。
