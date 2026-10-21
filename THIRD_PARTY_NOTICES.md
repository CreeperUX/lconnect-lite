# 第三方声明

## tinyuz 与 HDiffPatch（vendor/）

- `vendor/tinyuz`：https://github.com/sisong/tinyuz （提交 1d74ffa），MIT，版权 housisong，许可证见 `vendor/tinyuz/LICENSE`。
- `vendor/HDiffPatch`：https://github.com/sisong/HDiffPatch （提交 3b9dca7），MIT，许可证见 `vendor/HDiffPatch/LICENSE`。只收录了 tinyuz 编译所需的头文件、`libdivsufsort` 和 `libParallel`。
- `vendor/tuz_wrapper.cpp` 与 `crates/lcl-devices/tests/`（tinyuz 参考向量）来自 lian-li-linux（MIT，见下文）。

## PawnIO（vendor/pawnio/）

读 CPU 温度和功耗用的内核驱动，由传感器服务 lcl-sensord 通过设备 IOCTL 接口调用。

- `PawnIO_setup.exe` 2.2.0：https://github.com/namazso/PawnIO.Setup ，官方签名的驱动安装程序，原样随安装包分发。
  驱动本体 https://github.com/namazso/PawnIO 按 GPL-2.0-or-later 授权（含“只通过设备 IOCTL 接口通信的程序不受约束”的例外），
  源码见该仓库。
- `IntelMSR.bin`、`AMDFamily17.bin`（模块 0.2.11）：https://github.com/namazso/PawnIO.Modules 的编译产物，
  LGPL-2.1，许可证全文见 `vendor/pawnio/COPYING-modules.txt`（安装目录 `pawnio/COPYING-modules.txt`），源码见该仓库。
  模块嵌在 `lcl-sensord.exe` 里，原样加载，没有修改。

## lian-li-linux

以下部分移植自 lian-li-linux（只改了模块路径和少量适配）：

- `crates/lcl-effects/src/`（除 `lib.rs` 和 `h2/mod.rs` 里的 `parameters()`）：灯效渲染，
  来自 `crates/lianli-media/src/rgb` 与 `crates/lianli-shared/src/rgb.rs`。
- `crates/lcl-devices/src/` 的协议逻辑（见下）。

`src/h2.rs` 和 `src/wireless.rs` 中的协议逻辑（DES 帧封装、CRC、0xFA/0xFB 命令、
RX 设备记录解析、RF 分片、PWM/AIO 参数包、泵转速映射表）移植自 lian-li-linux：

https://github.com/sgtaziz/lian-li-linux （提交 f55f3e5）

```
MIT License

Copyright (c) 2026 sgtaziz

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```
