# Open HUB 0.3.2 — VXE / VGN 首批适配

复用 ATK 已实现的 COMPX 报告 08 / FF02:0002 传输，新增六个精确型号身份。VXE 与 VGN 独立列入配置导入菜单和配置品牌筛选；共享 USB 厂商 ID 不能作为品牌判断依据。运行时核对 CID 与 MID，并固定本次连接的身份。

|型号|USB VID:PID|CID / MID|按键|指示灯|
|---|---|---|---|---|
|VXE 蜻蜓 R1 · USB|3554:F58F|2 / 11, 12|6|关闭/常亮|
|VXE 蜻蜓 R1 Pro · USB|3554:F58C|2 / 7, 8|6|关闭/常亮|
|VXE MAD R · USB|373B:103F|2 / 49|5|未开放|
|VXE MAD R+ · USB|373B:104B|2 / 54|5|未开放|
|VGN 蜻蜓 F1 Pro · USB|3554:F502|2 / 1, 2|6|关闭/常亮|
|VGN 蜻蜓 F1 Pro Max · USB|3554:F506|2 / 3, 4|6|关闭/常亮|

本版保守开放 100–30000 DPI、50 DPI 步进、最多八档 XY，125/250/500/1000 Hz；不声明其他接收器、高回报率或所有无线版本。配置读取/导入、已解析按键、键盘点按、三种播放方式的板载宏沿用生产协议路径。MAD R/R+ 不显示不存在的第六个物理键，未确认的灯光不开放。R1/F1 只开放当前 DPI 档位的指示灯关闭/常亮，不宣称完整 RGB 律动。

共享 PID 的 R1 SE、R1 Pro Max、F1 MOBA 等 MID 不在本版名单，会在 EEPROM 读取/写入前拒绝。FF04/FF05、BITMOUSE 也不认领。新型号仅有生产驱动 + 内存 HID 回归和浏览器模拟边界测试，**没有这些鼠标实机**。只能确认模拟协议行为，不能认定整个 VXE/VGN 品牌兼容。

板载读取导入先预览后保存，不自动写入；不是官方配置文件解析器。无法完整解析的原厂按键/宏保持未知状态，不默认覆盖。板载宏的 70 事件限制、空槽保护、上传读回、失败恢复及键盘点按边界见 ATK-ADAPTER.md。

协议事实来自 ATK HUB 官方公开型号注册与 COMPX 存储布局：[官方 3.2.27 脚本](https://bpcdn.atkgear.com/hub-v3/production/3.2.27/static/index-DBE__Npj.js)，SHA256 `71a1ba1f86721609328c02ab3e69bfe8acf9e1aed84f328a94eeb1b2955b64b5`。沿用独立 Rust 实现，不分发官方 JS、官方运行库或厂商控制中心。型号表为 src/lib/compx-models.json，由 Rust 与 TypeScript 共同使用。
