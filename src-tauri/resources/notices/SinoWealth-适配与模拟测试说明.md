# SinoWealth 第一批原生适配（Open HUB 0.3.0）

这是实际 HID 协议适配代码，仍为未实机验证的实验版本。仅指定 VID/PID、固件和传感器组合会进入原生界面；未知设备不会尝试套用协议。当前用户只有迈从，以下新增型号均只完成模拟测试。

|品牌|型号|VID:PID|固件|按键|
|---|---|---|---|---|
|Glorious|Model O/O-|258A:0036|V103|6|
|Glorious|Model D|258A:0033|V102|6|
|Glorious|Model O Eternal|3794:A000|1009|6|
|Glorious|Model O 旧固件|258A:0027|V102|6|
|G-Wolves|Hati HT-M Wired|258A:0027|V161|6|
|DreamMachines|DM5 Blink|258A:0027|3106|8|
|Genesis|Xenon 770|258A:0027|3110|14|
|Machenike / 机械师|M620|258A:0029|V127|6|
|Mad Dog|GM905|258A:0029|V287|14|
|T-Dagger|Imperial T-TGM310|258A:0051|2642|8|
|Marvo|Scorpion G961|258A:1007|9677|6|

## 已实现

- 原生 HID 查询、型号/固件/序列/传感器复核；热插拔重新检测。
- 最多8档 DPI、独立XY、100步进、活动档位、125/250/500/1000Hz；实际传感器上限再次检查。
- 读取/编辑/写入原生按键：鼠标按钮、滚轮、DPI切换、键盘快捷键和修饰键；保留至少一个左键。
- 板载宏报文编码、有限重复、键盘/鼠标按下释放、1–255ms延迟、最多168事件；检查成对释放。界面可录制键盘事件、手动编辑和绑定宏。
- 持久化完整配置，按品牌/型号/固件隔离，支持 Open HUB 完整配置包预览、导入、导出、再次编辑和应用。
- 从鼠标当前板载数据导入配置中心：能迁移官方软件已经写入设备的 DPI、回报率和按键引用。不是直接解析各厂商私有配置文件。
- 配置中心的“离线设备方案”可打开以上型号完整UI，并调用不创建HidApi/Usb的合成设备模拟器；没有对应硬件时不能应用到当前迈从鼠标。
- 写前备份可读原始数据与本程序宏记录，写后读回DPI/按键；失败尝试恢复并核对。备份保存在 `%LOCALAPPDATA%/Open HUB/backups`。

## 真实限制

该系列公开协议不能读回宏内容。软件只显示自己保存的宏记录，不显示/伪造原厂宏内容。未知、已被引用的宏槽不允许覆盖；新宏可能写入空闲槽。若发生写入故障，DPI和按键会尝试恢复，新的未绑定宏数据可能留在槽位，原宏内容无法完整备份。宏报文成功发送不代表已验证物理执行。

这些型号尚未验证 Windows HID 接口、Feature Report 的实际返回长度、固件行为、物理按键与宏输出；模拟器使用合成报文，不能替代真实设备测试。长度/布局不符合白名单时拒绝写入，不按猜测裁剪数据。

Razer Synapse、Corsair iCUE、SteelSeries GG 等其他品牌的私有导入与完整原生协议仍未完成。不能把本批8品牌型号扩展说成“所有鼠标品牌均已支持”。厂商私有文件（XML/二进制/云配置）未解析的仍不支持，Open HUB 配置包不是它们的直接替代格式。

罗技、迈从和OpenRGB灯光的已有路径保留；新适配不会把共享VID视为同一品牌，也不会把迈从/罗技报文发送到这些设备。

## 协议依据和许可

从以下 MIT 许可公开实现核对数据结构、命令与型号表，独立实现 Rust 传输和界面。读取日期：2026-10-22。

- [libratbag driver-sinowealth.c](https://github.com/libratbag/libratbag/blob/master/src/driver-sinowealth.c)
- [libratbag 设备记录](https://github.com/libratbag/libratbag/tree/master/data/devices)
- 完整参考文件和原始许可附在安装目录 `notices/libratbag-sinowealth-reference.c` 与 `libratbag-MIT-license.txt`。

测试源码：`src-tauri/src/sinowealth.rs`（协议、故障、隔离、模拟器）；`scripts/test-sino-ui.cjs`（模拟设备界面）；原有配置、迈从和灯光测试继续运行。
