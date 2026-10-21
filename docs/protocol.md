# Lian Li L-Connect 3 (v2.1.29) USB 协议说明：SL-V3 无线收发器 + HydroShift II LCD-S

> 目的：为自有硬件编写轻量、可互操作的替代程序。本文只覆盖最小替代所需的部分。
> 来源：反编译的 C#（`lc3re\src`）以及 `lianli.lcd207.dll` 的 IL（这个程序集没有被反编译，我用 System.Reflection.Metadata 自写了一个 IL 转储工具来读它）。
>
> 标注约定：
> - **[读]**：直接从代码/IL 读出。
> - **[推]**：根据代码推断，固件行为未经实测。
> - **[?]**：不清楚，需要抓包或实测确认。
>
> 引用格式为 `文件: 方法`。`lcd207` 中的类（`WinUsbH2S`、`H2SController`、`Util`）只有 IL，没有 .cs 源文件。

---

## 0. 结论速览

| 设备 | VID:PID | 传输方式 | 端点 | 包大小 | 加密 |
|---|---|---|---|---|---|
| 无线 TX 发射器 "SLV3TX" | 0416:8040（V2 版本为 1A86:E304） | WinUSB（LibUsbDotNet） | OUT 0x01，IN 0x81（中断） | 64 B | 无 |
| 无线 RX 接收器 "SLV3RX" | 0416:8041（V2 版本为 1A86:E305） | WinUSB | OUT 0x01，IN 0x81（中断） | 64 B（读取时拼接多个 64 B） | 无 |
| HydroShift II LCD-S 水冷头 | 1CBE:A034 | WinUSB | OUT 0x01，IN 0x81（中断，缓冲区 512） | 512 B 命令头 + 可选负载 | 命令头使用 DES-CBC，key=IV=`"slv3tuzx"`；**设备应答为明文** |

- 风扇调速、读取转速和读取水温**都不需要原生 DLL**，全部是托管代码里的字节操作。
- RGB 灯效数据要用 **tinyuz** 压缩。`yuz.dll` 是 sisong/tinyuz 的 DLL 封装（字符串中可见 `dllTuz.pdb`、`tuz_compress`、`TuzEnc/TuzDec`），可以直接改用开源的 tinyuz。
- H2S LCD 的图片推送（JPEG）也是纯托管代码（DES + 原始 JPEG 字节）。只有 H.264 视频路径会用到 FFmpeg。`yccDrvV3.dll`、`cled.dll`、`SMBCtrl.dll` 只用于主板（ITE Super I/O / PCH）灯光，**与 Lian Li 的这些设备无关**。

---

## 1. 设备发现与打开

### 1.1 TX/RX 无线收发器 [读]
- `WinUsb.cs: GetRFSender / GetRFReciver`：遍历 `UsbDevice.AllDevices`，只匹配 `WinUsbRegistry`（即 WinUSB 驱动）并且 VID/PID 等于 0x0416/0x8040（TX）或 0x0416/0x8041（RX）的设备。也接受 0x1A86/0xE304 和 0x1A86/0xE305（`KnownDevices.cs` 中的 `WirelessControllerTransmitterV2/ReceiverV2`）。
- `WinUsb.cs: InitRFDev`：`OpenEndpointWriter(Ep01)`，`OpenEndpointReader(Ep01, 512, Interrupt)`，对应 OUT 0x01 和 IN 0x81。
- `WinUsb.cs: RfSend`：每次写 64 字节，超时 100 ms。连续失败 5 次后，通过**另一个**收发器发送复位命令（见 2.1 中的 0x15）。
- `WinUsb.cs: RfRead / ReadAll`：先 `Flush`，然后反复以 10 ms 超时读取 64 字节块，直到超时，再把这些块拼接起来。如果首字节为 0，视为失败。
- 服务层（`MainService.cs: addConnectedController`）会给 TX 构造一个 `HidApiHidDevice` 对象，但实际 I/O 全部通过 `RFController` 和 LibUsbDotNet 的 WinUSB 完成。[推] 这个 HID 对象只用于识别设备和管理生命周期。
- 代码中没有安装驱动的调用（`WinUSBDriverManager.cs` 没有被任何地方引用）。[推] 设备通过 MS OS 描述符自动绑定 WinUSB；在 Linux 上可以直接用 libusb。

### 1.2 HydroShift II LCD-S [读]
- `lcd207 WinUsbH2S: GetDevice`：匹配 `WinUsbRegistry` 且 PID=0xA034、VID=0x1CBE 的设备。
- `WinUsbH2S: InitDev`：`OpenEndpointWriter(1)`，`OpenEndpointReader(0x81, 512, 3=Interrupt)`；然后调用 `SetFrameRate(30)`（命令 0x0F，参数 30）；初始化 `aioParamsTX = {2080, 80, 80, 80}`；启动后台发送循环（`StartSendMsg`，见 4.4）。
- `H2SController: Init` 还会检查 1A86:AD22（`HydroShiftIILCDSquareDisplayMode`，即“桌面/副屏模式”，走 HID）。最小替代可以忽略这个设备。

---

## 2. 无线（RF）协议：TX / RX 收发器

整体结构 [读]：**TX 只负责下发命令，RX 只负责上报设备列表和状态。** 所有对风扇或水冷头的命令都是 240 字节的“RF 负载”，由 TX 拆成 4 个 64 字节的 USB 包发出。

### 2.1 USB 层命令（64 字节包，首字节为命令号）
来源：`USB_CMD.cs`，以及 `MasterDevice.cs` 中各个方法。

| 命令 | 发给 | 作用 | 来源 |
|---|---|---|---|
| 0x10 | TX | 发送一段 RF 负载（见 2.2） | `MasterDevice.cs: SendRfData` |
| 0x10 | RX | 拉取设备列表和状态（见 2.4） | `MasterDevice.cs: GetDev / RefreshList` |
| 0x11 | TX | 查询主机 MAC、时钟和 TX 固件版本（见 2.3） | `MasterDevice.cs: QuerryMasterMac` |
| 0x12 | — | `USB_SendHeart`：枚举中有定义，**代码里从未使用** | `USB_CMD.cs` |
| 0x15 | TX 或 RX | 复位“另一个”收发器：发给 TX 会复位 RX，发给 RX 会复位 TX | `MasterDevice.cs: ResetRx / ResetTx` |
| 0x16 | TX 或 RX | 关闭 USB | `MasterDevice.cs: CloseRecUsb / CloseSenderUsb` |
| 0x20-0x25、0x0E | — | 固件升级相关，最小替代不需要 | `USB_CMD.cs` |

### 2.2 TX：0x10 发送 RF 负载 [读]（`MasterDevice.cs: SendRfData`）
每个 240 字节的 RF 负载拆成 4 个 USB 包，每包携带 60 字节：

| 偏移 | 长度 | 含义 | 示例 |
|---|---|---|---|
| 0 | 1 | 0x10 | `10` |
| 1 | 1 | 分片序号 0..3 | `00` |
| 2 | 1 | 信道：目标设备的**当前**信道（`rf.channel`）；广播时用 `MasterChannel` | `08` |
| 3 | 1 | rx_type：目标设备的**当前**槽位（`rf.rx_type`）；广播时为 0xFF | `03` |
| 4 | 60 | RF 负载的第 [60·n, 60·n+60) 字节 | |

### 2.3 TX：0x11 查询主机 MAC [读]（`MasterDevice.cs: QuerryMasterMac`）
- 请求：`[0]=0x11, [1]=MasterChannel`（默认 8），其余字节为 0，总长 64。
- 应答（64 字节）：

| 偏移 | 含义 |
|---|---|
| 0 | 0x11 |
| 1..6 | TX 的 MAC，即“主机 MAC”，后续所有命令都会用到 |
| 7..10 | 时钟计数，大端 u32；乘以 0.625 得到毫秒数（`SysClock`） |
| 11..12 | TX 固件版本，大端 u16 |

如果 MAC 全为 0 或时钟为 0，按失败处理。软件**每秒调用一次**（在 `Run` 循环中）。

### 2.4 RX：0x10 读取设备列表 [读]（`MasterDevice.cs: GetDev / RefreshList / GetRFList`）
- 请求：`[0]=0x10, [1]=页数`（每页 10 台设备，初始为 1）。开启主板同步（FgSync）时，`[2..3]` 填第一台设备的风扇转速（大端），[推] 用于向主板回报转速。
- 读取长度为 `434 × 页数` 字节（由多个 64 B 中断包拼接）。
- 应答头：

| 偏移 | 含义 |
|---|---|
| 0 | 0x10 |
| 1 | 设备数 N（包括其他 TX/主机） |
| 2..3 | 如果 `[2]` 的 bit7=1：RX 固件版本 = `((b2&0x7F)<<8)+b3`。否则表示主板 PWM 采样：low=`b2&0x7F`、high=`b3`，主板占空比 = `255·high/(high+low)` |
| 4.. | 每台设备一条 42 字节记录 |

- 每条设备记录 42 字节，从偏移 4 开始，步长 42。只有 `rec[41]==0x1C` 时才是有效记录：

| 记录偏移 | 长 | 字段 | 说明 |
|---|---|---|---|
| 0 | 6 | 设备 MAC | 设备的唯一标识，字符串形式为 `aa:bb:..` 小写 |
| 6 | 6 | 已绑定的主机 MAC | 全 0 表示未绑定；等于自己 TX 的 MAC 表示已绑定到本机 |
| 12 | 1 | channel | 当前信道 |
| 13 | 1 | rx_type | 当前槽位（1..13） |
| 14 | 4 | 设备时钟，大端，单位 0.625 ms | |
| 18 | 1 | dev_type | 0=风扇组；1..9=Strimer；10=HydroShift(H2)；**11=HydroShift II（H2S，WaterBlock2）**；65=LC217；66=V150；0xFF=其他主机/TX |
| 19 | 1 | fan_num | 风扇数量；≥10 时减 10，并表示 INF 右侧挂接 |
| 20 | 4 | effect_index | 当前灯效 ID，用作灯效下发的“确认”（见 2.7） |
| 24 | 4 | fans_type[4] | 每个风扇的型号码（见下表）；**水冷头的 fans_type[3] 是水温**（`RFController.cs: GetAioTemp`） |
| 28 | 8 | fans_speed：4 个大端 u16 | 转速 = `((hi&0x0F)<<8)|lo`。高字节的高 4 位是标志位：byte28 bit7=同步线已接、bit6=主板灯光同步、bit5=PWM 线已接、bit4=FG；byte30 高 4 位=WirelessThemeSwitch；byte32 和 byte34 的高 4 位组成 RF 固件版本 |
| 36 | 4 | fans_pwm[4] | 设备当前 PWM（0..255）；4 个都为 0 且有转速时，软件按 100 处理 |
| 40 | 1 | cmd_seq | 设备已执行的控制命令序号（见 2.6） |
| 41 | 1 | 0x1C | 有效标记 |

- 水冷头（dev_type 10 或 11）的读数（`LWirelessController.cs: handleGetHydroShiftIILCDHandshakeInfo`）[读]：风扇转速 = `Speeds[0]`，**泵转速 = `Speeds[3]`**（最后一项），水温 = `fans_type[3]`，风扇数 = `fan_num`。
- fans_type 型号码（`RfDevice.cs: InitAttr`）[读]：20–26=SL V3；27–35=TL V2；36–39=SL INF；40=RL120；**41/42=CL V1（42 为反叶）**；43–50=SL INF V3；51–58=TL V3；59–62=SL V4；63=P28 V2；**126/127=CL V2（127 为反叶）**。`(n&1)` 等奇偶规则见源码。
- 存活计数（`MasterDevice.cs: RefreshList / FindDev`）：每轮轮询把 `live` 减 1，再次看到设备时重置为 30。小于等于 0 时把设备从列表中删除（锁定模式下不删除）。

### 2.5 RF 负载通用头（240 字节）[读]
所有 TX 下发的 RF 负载都使用这个头（`MasterDevice.cs: SyncControlInfo / SyncPwm / SendAioInfo / SyncRgbData / SaveConfig`）：

| 偏移 | 长 | 含义 |
|---|---|---|
| 0 | 1 | 0x12（固定值） |
| 1 | 1 | RF 子命令（见下表；对应 `RF_CMD.cs`） |
| 2 | 6 | 目标设备 MAC；广播时为 FF×6 |
| 8 | 6 | 主机 MAC。绑定或调速命令中填 `target_master_mac_addr`：绑定时等于本机 TX 的 MAC，解绑时为全 0 |
| 14 | 1 | 目标 rx_type（RgbSync 中此处另作他用，见 2.7） |
| 15 | 1 | 目标信道 |
| 16 | 1 | 子命令相关（slaveIndex 或 0） |
| 17.. | | 子命令参数 |

| 子命令 | 名称 | 参数 | 何时发送 |
|---|---|---|---|
| 0x10 | Bind / 设置 PWM | `[16]`=序号 b（本机已绑定设备中的第几个，从 1 开始；解绑时为 0），`[17..20]`=4 路目标 PWM | 绑定状态不一致时在每次循环中发送；PWM 偏差 >5 时每秒发送 |
| 0x12 | Select（定位闪烁） | `[17]`=目标 cmd_seq | 重复 10 次 |
| 0x14 | 时钟同步（广播） | `[14..233]`=220 字节 cpuInfoParam（LCD 风扇使用；`[14+32..14+38]` 为年(BE16)、月、日、时、分、秒） | 每秒一次，rx=0xFF |
| 0x15 | 保存配置（广播） | 无 | 绑定后 5 秒内每轮发送；之后满 1 小时 / 3 小时发送一次 |
| 0x16 | 重启 LCD | `[17]`=seq | |
| 0x19 | AIO 切换到无线主题 | `[17]`=seq | |
| 0x20 | **RgbSync 灯效数据** | 见 2.7 | effect_index 不一致时发送 |
| 0x21 | **AIO 参数（水冷头）** | `[18..49]`=32 字节 aio_param（见 3.2） | 每秒一次，只发给已绑定的 dev_type 10 或 11 |
| 0x22 | AIO 图片 | 分片，每片 220 B | 只用于无线 LCD |
| 0x23/0x24/0x26/0x27/0x28/0x29 | 关闭 Wi‑Fi / 主板同步开关 / 灯光开关 / TL V3 灯光 / 下发颜色 / 播放主题 | `[17]`=seq，`[20]`=参数 | 最小替代不需要 |

### 2.6 绑定、解绑、“锁定绑定”、信道与寻址
寻址方式 [读]：**设备由 6 字节 MAC 标识。** 无线发送时还必须带上设备的**当前** channel 和 rx_type（USB 包的 `[2]` 和 `[3]`），见 `SendRfData` 的调用参数 `rf.channel, rf.rx_type`。

**绑定流程** [读]（`RFController.cs: Bind`、`RfDevice.cs: bind`、`MasterDevice.cs: SyncControlInfo`）：
1. 从 RX 列表中找到 `master_mac == 00..00` 的设备（状态为 None）。
2. 主机端设置：`target_master_mac = 本机 TX MAC`；`target_channel = MasterChannel`；`target_rx_type = GetRxUnused()`，即 1..13 中第一个空闲的槽位（`MasterDevice.cs: GetRxUnused`）。
3. 在每轮循环里，只要设备上报的 `rx_type/channel/master_mac` 与目标不一致，就通过 TX 发送 RF 子命令 0x10。字段为：`[2..7]`=设备 MAC，`[8..13]`=本机 MAC，`[14]`=目标 rx_type，`[15]`=目标信道，`[16]`=序号 b，`[17..20]`=目标 PWM。各设备之间间隔 5 ms。
4. 设备在 RX 列表中报告 `master_mac == 本机` 后即为 BindLink。之后 5 秒内持续广播 0x15 SaveConfig。
5. 软件重启后，`master_mac == 本机` 的设备会被自动认领为已绑定（`RefreshList` 中 `flag2` 分支），**不需要重新绑定**。[推] 绑定关系保存在设备的固件或闪存中。

- **解绑** [读]（`RfDevice.cs: unBind`）：`target_master_mac = 00×6`、`target_rx_type = 0`、`[16] = 0`，同样通过子命令 0x10 下发。
- **数量限制** [读]（`RFController.cs: Bind`）：总共不超过 10 台，风扇不超过 10 台，Strimer 不超过 3 台，H2 和 H2S 各不超过 1 台。`RefreshList` 中如果已绑定设备达到 12 台，会自动解绑新设备。
- **“锁定绑定”（Lock）** [读]（`RFController.cs: LockDevice / CheckLockAndInitData`）：**纯主机端功能，不向设备发送任何命令。** 把本机已绑定设备的列表写入 `savedDevices.config`，并设置 `lock_list=true`。效果：`RefreshList` 不再加入新设备、不再删除离线设备、不再检查信道冲突。
- **信道** [读]：
  - `MasterChannel` 默认为 8，自定义值保存在 `<主机MAC>.config`（`RFController.cs: GetChannel`）。
  - 自动避让（`MasterDevice.cs: CheckChannelConflict`）：把所有主机按 MAC 排序，第 i 个使用 `8+4i`。
  - `RFController.cs: ChangeChannel` **拒绝偶数信道**，这与自动分配使用偶数（8、12、…）的行为互相矛盾，原因 [?]。
  - 切换信道时，把所有已绑定设备的 `target_channel` 改为新值，然后由子命令 0x10 推送下去。
  - [?] RX 如何得知信道，代码中没有体现。TX 在 0x11 查询中会收到信道。

### 2.7 灯效下发：RF 子命令 0x20（RgbSync）[读]（`MasterDevice.cs: SyncRgbData`）
灯效在**主机端逐帧渲染**为 RGB 数据，用 tinyuz 压缩后分片下发。设备上并没有可以用编号直接调用的“内置特效”。[推] 静态颜色也必须按这种方式下发帧数据。

数据格式 [读]：`rgb_data` 依次为“帧 → LED → R,G,B”，每帧 `3 × led_num` 字节（`RgbEffect.cs: GetRgbData`，`RgbBpp=3`）。然后调用 `Util.CompressYuz`，即 `yuz.dll!TuzEnc`（tinyuz）。压缩后最大 12288 B（`MasterDevice.LzoMaxRgbDataLen`）。

分包规则：第 0 包为头，之后每包携带 220 B 数据；总包数 = `ceil(len/220) + 1`。

| 偏移 | 长 | 第 0 包（头） | 第 k≥1 包 |
|---|---|---|---|
| 0..1 | 2 | `12 20` | `12 20` |
| 2..7 | 6 | 设备 MAC（全部同步时为 FF×6） | 同左 |
| 8..13 | 6 | target_master_mac（本机 MAC） | 同左 |
| 14..17 | 4 | effect_index（大端 u32，取当天零点以来的毫秒数，作为唯一 ID；`Util.GetTimeStamp`、`RgbEffect.RefreshIndex`） | 同左 |
| 18 | 1 | 包序号 0 | 包序号 k |
| 19 | 1 | 总包数（含头） | 同左 |
| 20..23 | 4 | 压缩后数据长度（大端 u32） | 数据第 [(k-1)·220, k·220) 字节 |
| 24 | 1 | 0 | ↑ |
| 25..26 | 2 | total_frame（大端） | ↑ |
| 27 | 1 | led_num | ↑ |
| 32..33 | 2 | interval 整数部分（大端，[推] 单位为毫秒/帧） | ↑ |
| 34 | 1 | interval 小数部分 ×100 | ↑ |
| 35..36 | 2 | sub_interval（大端） | ↑ |
| 37 | 1 | isOuterMatchMax | ↑ |
| 38..39 | 2 | total_sub_frame（大端） | ↑ |

- 时序：头包先发 1 次，再间隔 20 ms 补发 3 次，然后依次发送数据包。
- **确认机制**：设备在 RX 列表中报告的 `effect_index` 与下发值一致，即表示灯效已经生效；不一致就整段重发（`SyncRgbData` 开头的比较）。
- **CL 风扇的灯珠布局** [读]（`RgbEffect.cs: CLMode3 / CombingCLRgbData`）：每个风扇 24 颗 LED，依次为 `[0..7]` 中心（Center）、`[8..23]` 外圈（Outer，16 颗）。`led_num = FanNum × 24`。
- **CL 静态色**（`CLMode3`）[读]：生成 30 帧完全相同的帧，`interval = SpeedType × 20`。SpeedType 取值 L1..L5 = 7、6、5、4、3，默认 L3=5，所以 interval=100。亮度通过 `颜色 × bright >> 8` 实现，bright 的 L1..L5 = 0、64、128、192、255。
- **主机端限功率** [读]（`RgbEffect.cs: initUserColor`）：如果 R+G+B > 600（CL）或 > 570（H2 泵头），会反复乘以 0.95，直到不超过上限。[推] 这是为了限制电流，替代程序应当保持同样的处理。
- **水冷头 RGB**（`RfDevice.cs: SetWaterBlockEffect`，`RgbEffect.cs: GetWaterBlockEffect / H2Mode3`）[读]：led_num = `24 + FanNum×24`。前 24 颗是泵头灯环，后面依次是各个 CL 风扇（每个 24 颗，布局同上）。泵头静态色同样是 30 帧，`interval = SpeedType × 20`。
- [推] 最小可行的静态色方案：为 `led_num` 颗 LED 生成 30 帧相同的颜色，用 tinyuz 压缩，然后发送 0x20。[?] 只发 1 帧固件是否接受，未经验证。

---

## 3. 风扇与水泵调速（无线路径）

### 3.1 风扇 PWM [读]
- API 链路：`LWirelessDevice.cs: SetFanSpeed(mac, duty0..255)` → `RFController.cs: SetFansRPM`。4 路使用相同的值，先写入 `fans_pwm_list`；`RfDevice.cs: SetFansPwmFromAvg` 对最多 10 个样本取平均，得到 `target_fans_pwm`。
- 下发：`MasterDevice.cs: SyncPwm` 每秒执行一次。只有在 `|设备上报的 fans_pwm − target| > 5` 时，才发送 RF 子命令 0x10，PWM 放在 `[17..20]`。
- **PWM 字节的取值范围是 0..255。** 服务层用 `Map(%, 0..100 → 0..255)` 换算（`LWirelessController.cs: startWriteFanSpeed`）。CL 的最小占空比为 10%。
- **保留值** [读]，含义为 [推]：
  - **6**：`SetFansRPM` 会把 6 改成 0。主板同步时，对 MAC 末字节为 `e1` 的设备写入 `{6,6,6,6}`（`RFController.cs: SetAllFansRPMSyncMainBoard`），所以 6 很可能表示“跟随主板 PWM 线”。
  - **153–155**：对 CL 风扇，153 和 154 被改为 152，155 被改为 156（`MasterDevice.cs: NeedSyncPwm`）。替代程序应避开这三个值。
- 默认值：新建设备时 target_fans_pwm 为 100（`RfDevice.cs` 构造函数）。
- 示例：把 MAC 为 `11:22:33:44:55:66` 的设备设为 50%（=127）。本机 MAC 为 `AA..FF`，设备当前信道 8、rx 3、序号 1。RF 负载为 `12 10 11 22 33 44 55 66 AA BB CC DD EE FF 03 08 01 7F 7F 7F 7F 00…`（共 240 B），由 TX 拆成 4 个包发送，第一个包是 `10 00 08 03 <负载 0..59>`。

### 3.2 无线水泵（H2S 已绑定到 TX 时）[读]
- `RFController.cs: SetH2SAioPumpSpeed(mac, rpm)` 把转速写入 `aio_param[28..29]`（大端），然后由 `MasterDevice.cs: SendAioInfo` 每秒通过 RF 子命令 0x21 发送，参数放在 `[18..49]`。
- **H2S 泵转速到写入值的映射**（USB 路径使用同一张表，见 4.3）。转速先限制在 1600–3200 rpm，再按第一个满足的区间计算：

| rpm 区间 | 写入值 |
|---|---|
| 1600–1800 | 1590 − (rpm−1600)·0.95 |
| 1800–2000 | 1400 − (rpm−1800) |
| 2000–2200 | 1200 − (rpm−2000) |
| 2200–2400 | 1000 − (rpm−2200) |
| 2400–2600 | 800 − (rpm−2400) |
| 2600–2800 | 580 − (rpm−2600)·1.11 |
| 2800–3000 | 330 − (rpm−2800)·1.2 |
| 3000–3200 | 90 − (rpm−3000)·0.45 |

  示例：2000→1200（0x04B0），2400→800（0x0320），3200→0。[推] 写入值越小，泵越快。H2（WaterBlock，dev_type 10）使用另一张 1600–2500 rpm 的表（`SetAioPumpSpeed`）。
- aio_param 共 32 字节（`RFController.cs: SetAioParams`）：

| 偏移 | 内容 |
|---|---|
| 0–3 | CPU 温度、CPU 负载、GPU 温度、GPU 负载（均 ≤99） |
| 4–5 | 风扇转速（大端，只用于 LCD 显示） |
| 6 | 轮播间隔 |
| 7–12 | 泵、CPU 温度、CPU 负载、GPU 温度、GPU 负载、风扇转速各项的显示开关 |
| 13–24 | 三组 ARGB 颜色 |
| 25 | 亮度（默认 80） |
| 26 | 固定为 1（选择主题时为 0） |
| 27 | 主题索引 |
| **28–29** | **泵转速写入值（大端）** |
| 30 | 旋转 |
| 31 | 0 |

- 无线模式下，H2S 的冷排风扇**不是独立的 RF 设备**，而是用水冷头 MAC 的 0x10 PWM 来控制。服务层用 MAC 为键登记了一个 `CLFan120LED` 风扇配置，MaxSpeed=2150（`LWirelessController.cs: addSettingDevice`）。

---

## 4. HydroShift II LCD-S 的 USB 直连协议（`lcd207.WinUsbH2S`，只有 IL）

### 4.1 命令帧 [读]（`WinUsbH2S: GetBaseCmdBuf / Encrypt / Send`）
1. 先构造 500 字节的明文：

| 偏移 | 内容 |
|---|---|
| 0 | 命令号 |
| 1 | 0 |
| 2 | 0x1A |
| 3 | 0x6D |
| 4..7 | 时间戳：程序启动以来的毫秒数，小端，取 Int64 的低 4 字节 |
| 8.. | 参数 |

2. 用 DES-CBC 加密，key=IV=`73 6C 76 33 74 75 7A 78`（"slv3tuzx"），[推] 填充方式为 .NET 默认的 PKCS7。500 字节加密后得到 504 字节。
3. 放入 512 字节缓冲区的开头，并设置 `[510]=0xA1`、`[511]=0x1A`。
4. 带额外负载的命令（0x65 JPEG、0xFC RGB）直接把原始负载**不加密**地附加在 512 字节头之后，整个缓冲区一次写出。
5. 发送和读取（`SendAndRead`）：写超时 200 ms；写入成功后读取 512 字节，超时 2000 ms。**代码不对应答做解密**（`WinUsbH2S: Read`），通常 `[0]` 回显命令号（`GetVer` 会检查 `[0]==0x0A`）。

### 4.2 命令号（`lcd207.CmdType`）[读]
| 编号 | 名称 | 编号 | 名称 |
|---|---|---|---|
| 0x0A | GetVer | 0x65 | PushJpg（0x66 为 PNG） |
| 0x0B | Reboot | 0x79/0x7B | StartPlay/StopPlay（H.264） |
| 0x0D | Rotate | 0x96 | SwitchToDesktop |
| 0x0E | 亮度 | 0xF9 | SetWthemeIndex |
| 0x0F | SetFrameRate | **0xFA** | **GetH2Params（读转速和水温）** |
| 0x33/0x34 | SetClock/StopClock | **0xFB** | **SyncPumpFan（设置泵和风扇）** |
| 0x60–0x62 | GetTemperature/SetPumpSpeed/GetPumpSpeed（[?] H2S 路径没有使用） | **0xFC** | **PushRgbData** |

### 4.3 0xFB SyncPumpFan：设置泵转速和风扇占空比 [读]（`WinUsbH2S: UpdatePumpFan / SendPumpFan`，`Util: Compute`）
在命令帧明文的 `[8..23]` 放 16 字节参数：

| 参数偏移 | 内容 | 示例 |
|---|---|---|
| 0 | 0xFF | FF |
| 1 | 0x0F | 0F |
| 2 | 0xA2 | A2 |
| 3 | 0 | 00 |
| 4..5 | 泵转速写入值（大端，映射规则同 3.2 的 H2S 表） | 2400 rpm → `03 20` |
| 6 | 风扇 1 占空比，0..255 | 7F |
| 7 | 风扇 2 占空比 | 7F |
| 8 | 风扇 3 占空比 | 7F |
| 9..11 | 0 | 00 00 00 |
| 12..13 | 参数第 0..11 字节的 CRC-16，大端 | `7D 8A` |
| 14..15 | 0 | 00 00 |

- CRC 算法：多项式 0x1021、初值 0、MSB 优先。查表首项为 0000、1021、2042、3063，**即 CRC-16/XMODEM** [读]。示例值 0x7D8A 由我按该算法计算得出。
- 服务层（`HydroShiftIILCDController.cs: startUpdateFanPumpSpeedTimer`）每秒调用一次 `UpdatePumpFan(pump, d, d, d)`，三路风扇使用同一个值。d = `max(10%, 曲线rpm/MaxSpeed)`，再映射到 0..255。
- `UpdatePumpFan` 只更新缓存。真正的发送在后台循环中进行，约每 3.6 s 一次（见 4.4）。
- **互斥**：如果 RF 列表里有已绑定的 dev_type 10 或 11，`SendPumpFan` 会直接返回；服务层的 `isWirelessMode()` 为真时也会停止 USB 调速。**USB 和无线只能二选一。**

### 4.4 0xFA GetH2Params：读取泵转速、风扇转速、水温、MAC [读]（`WinUsbH2S: StartSendMsg` 循环，`H2SController: GetH2Params`）
请求不带参数。应答为 512 B 明文：

| 应答偏移 | 含义 | 来源 |
|---|---|---|
| 8 | ? | → result[0] |
| 9 | **风扇数量** | result[1]，即 FanCount |
| 10..12 | ? | result[2..4] |
| 13 | **水温（°C 整数）** | result[5] |
| 14..15 | **风扇转速**（大端） | result[7] |
| 16..17 | [推] 风扇 2 转速（大端） | result[8] |
| 18..19 | [推] 风扇 3 转速（大端） | result[9] |
| 20..21 | **泵转速**（大端） | result[10] |
| 22..27 | 水冷头 RF MAC | result[11..16]；用于判断是否已无线绑定 |
| 30 | ? | result[17] |
| 31 | RF 模块版本（RFVer） | result[18] |

后台循环时序（`WinUsbH2S/<>c__DisplayClass87_0/<<StartSendMsg>b__0>d: MoveNext`）[读]：
- 每 330 ms 从队列中取出并发送 1 条命令。队列中超过 10 条时，丢弃较旧的命令。
- 计数超过 6（约 2.3 s）时加入一次 0xFA；计数超过 10（约 3.6 s）时调用一次 `SendPumpFan`（0xFB）。
- 服务层另有定时器，每 2 s 读取一次缓存的 `aioParamsRX`（`HydroShiftIILCDController.cs: startUpdateHandshakeInfoTimer`）。

### 4.5 0xFC PushRgbData：H2S 的 USB 灯效 [读]（`WinUsbH2S: UpdateRgbData`）
- 负载 = `tinyuz 压缩数据 ‖ total_frame(BE16) ‖ interval(u8) ‖ led_num(u8)`。
- 命令帧明文 `[12..15]` = 负载长度（大端 u32），帧之后直接附加负载。
- 数据来源：`RFController.cs: GetH2RgbEffect` → `SetWaterBlockEffect`。LED 布局与 2.7 相同：24 颗泵头 LED，加上 FanNum×24 颗风扇 LED。interval 取 `ceil(interval)`，并截断为 1 字节。

### 4.6 其他命令
- 0x0A GetVer：应答 `[8..39]` 为 UTF‑8 版本字符串（`WinUsbH2S: GetVer`）。
- 0x0F 帧率：参数 `[8]=30`，打开设备时发送。
- 0x65 JPEG：`[8..11]` = 图片长度（大端），头之后附加原始 JPEG（`WinUsbH2S: PushImage`）。

---

## 5. 主板 PWM 同步与软件风扇曲线

- **软件曲线** [读]：
  - 风扇：`LWirelessController.cs: startWriteFanSpeed` 每 **1000 ms** 执行一次。读取 CPU、GPU 或水温（`SystemInformation`，数据来自 HWiNFO 等），按曲线算出 rpm，换算为占空比（%）再映射到 0..255，调用 `SetFanSpeed`。
  - 水泵：`startWritePumpSpeed` 每 1000 ms 调用一次 `SetPumpRPM`。
  - USB 直连的 H2S：`HydroShiftIILCDController.cs: startUpdateFanPumpSpeedTimer`，也是每 1000 ms。
  - 无线 PWM 实际的发出频率约为每秒一次（并且只在偏差 >5 时发送）。USB H2S 约每 3.6 s 发送一次。
- **主板同步（IsMotherboardRPMSync）** [读]：
  - 开启后，上面这些曲线定时器全部直接返回（不再计算和下发）。
  - `LWirelessDevice.cs: SetFanMotherboardSync` → `RFController.FgSync(true)`，并调用 `SetAllFansRPMSyncMainBoard`，对 MAC 末字节为 e1 的设备写入 `{6,6,6,6}`。
  - 此后在每次 RX 轮询（0x10）中，**主机读取 RX 采样到的主板 PWM**（应答 `[2..3]`，见 2.4），计算 `pwm = 255·high/(high+low)`，用 `SyncFgPwm` 写入所有设备的 target，再由 SyncPwm 下发。同时把第一台设备的转速通过请求的 `[2..3]` 回报给 RX。
  - 也就是说，**主板同步需要主机程序参与转发**（[推] 至少对大多数设备是这样）。关闭同步时，所有设备的目标值都会被设为 100。
- 主循环节奏（`MasterDevice.cs: Run`）：每轮约 100 ms 延时 + 灯效同步 + 200 ms 延时（正在变更灯效时为 20 ms）。距上次超过 500 ms 时轮询一次 RX；每 1000 ms 执行一次 SyncPwm、0x11 查询 MAC、0x21 AIO 参数和 0x14 时钟同步。

## 6. 心跳与主机停止发送后的行为
- [读] 无线路径和 H2S 路径**都没有专门的心跳命令**。0x12 `USB_SendHeart` 虽有定义但从未使用。LCD 类设备的“心跳”（`STRController.SendHeartBeat`）实际上就是 GetVer。
- 主机在运行期间会周期性发送以下数据，[推] 它们事实上起到了保活作用：
  - 0x14 时钟同步广播（1 Hz）
  - 0x21 AIO 参数（1 Hz）
  - 0x10 PWM（只在偏差 >5 时发送）
  - USB H2S 的 0xFA 和 0xFB（约 2.3 s 和 3.6 s 一次）
- [?] **固件在主机停止发送后的行为，从主机代码中无法得知**：是保持最后的 PWM 和灯效，还是回退到默认值或主板 PWM。有两点间接线索：
  1. 软件会定期广播 0x15 SaveConfig，并在绑定或修改灯效后保存，[推] 说明设备会把配置存起来，离线时继续使用。
  2. H2S 有 `SetOfflineClock`（离线时钟）功能。

  建议在实测中验证：拔掉主机程序后观察 1 分钟和 10 分钟时的风扇转速。

## 7. 原生 DLL 依赖
| DLL | P/Invoke 签名（摘录） | 用途 | 替代程序是否需要 |
|---|---|---|---|
| yuz.dll（tinyuz） | `uint TuzEnc(byte[] in, int inLen, byte[] out)`；`void TuzDec(byte[] comp, int compLen, int origLen, byte[] out)`（`Util.cs`） | RGB 帧压缩 | 需要，但可以改用开源 tinyuz。[推] 字典大小为 4 KB，依据是 `Util.CompressByCmd` 调用 `tinyuz.exe -c-4k`；[?] DLL 内部的默认值尚未确认 |
| yccDrvV3.dll | `gb_outp/gb_outpw/gb_outpd(uint port, …)`、`gb_inp/gb_inpw/gb_inpd(uint port)`（`NativeYccMethods.cs`） | 主板 Super I/O 端口读写 | 不需要 |
| cled.dll | `CLed_Init`、`SetTrueColorLedMode(uint)`、`SetLedColor(uint)` 等（`PchLedCtrl.cs`） | 技嘉 PCH 灯光 | 不需要 |
| SMBCtrl.dll | `dllexp_*`（`monocled/InvkSMBCtrl.cs`） | SMBus 内存或主板灯光 | 不需要 |
| hidapi.dll | 标准 hidapi | 其他 HID 设备 | 不需要 |
| lianli.lcd207.dll | **托管程序集**，唯一的 P/Invoke 是 gdi32 和 setupapi | H2S 协议实现 | 不需要（协议已在第 4 节描述）。H.264 播放会用到 FFmpeg |

## 8. 最小替代实现的步骤建议（[推]）
1. 打开 TX 和 RX（WinUSB，EP 0x01 和 0x81，64 B），以及 H2S（WinUSB，EP 0x01 和 0x81，512 B）。
2. 向 TX 发送 0x11 获取主机 MAC，然后每 300–500 ms 向 RX 发送 0x10，解析设备列表。
3. 对于 `master_mac == 本机` 的设备，直接使用其当前的 channel 和 rx_type。新设备按 2.6 的流程绑定。
4. 选择调速路径：
   - **USB**：H2S 不绑定到 TX。每 1–3 s 发送 0xFB 设置泵和风扇，每 2 s 发送 0xFA 读取泵转速、风扇转速和水温。
   - **无线**：H2S 已绑定到 TX。用 0x10 设置 PWM、用 0x21 设置泵；泵转速取 Speeds[3]，水温取 fans_type[3]。
5. 灯效：生成 30 帧颜色相同的帧，用 tinyuz 压缩。H2S 走 USB 时发送 0xFC，走无线时发送 RF 0x20，并等待设备上报的 effect_index 与下发值一致。
6. 不要使用 PWM 值 6 和 153–155；颜色的 R+G+B 不要超过 570–600。

## 9. 未决问题与风险
- **固件离线行为未知**（第 6 节）。
- `ChangeChannel` 的奇偶限制与自动分配逻辑互相矛盾；RX 如何得知信道也不清楚。
- tinyuz 的字典大小和流格式需要与固件解码器匹配。建议用官方软件抓一次 0x20 或 0xFC 的数据包，用 TuzDec 解压做对照。
- H2S 0xFA 应答中的 result[0]、[2..4]、[8]、[9]、[17] 含义未知。
- 0xFB 中三路“风扇”具体对应什么（[推] 可能是每个风扇各一路）尚未实测；官方软件总是给三路写相同的值。
