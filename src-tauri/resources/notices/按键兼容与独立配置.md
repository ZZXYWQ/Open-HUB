# 按键兼容与独立配置

Open HUB 仍是实验项目。识别品牌、显示设备图片或能改 DPI，都不等于已经支持全部自定义按键；也不能据同品牌其他型号推断兼容。

按用户选择，仅保留逐鼠标独立配置：按明确设备实例发送原生 HID 报文，软件宏按已核对的输入接口路由。不加入对所有鼠标同时生效的全局五键映射，不以全局拦截冒充设备独立控制。

## 当前实际范围（0.3.7）

|精确型号|可改物理控制|可执行动作|仍未实现|
|---|---|---|---|
|Razer Basilisk V3 Pro 1532:00AA / 00AB|右/中/两侧键、滚轮上/下、底部配置、拨片、左右倾斜、顶部前/后键共 12 控制|鼠标动作、八位修饰快捷键、禁用、DPI 循环、默认记录、单次/开关软件宏|原生 flash 宏、按住循环、Hypershift、其他 Basilisk；左键保护|
|Pulsar X2 V2 Mini 3554:F507 / F508|右、中、后退、前进|五种鼠标按键互换、DPI 循环/升降、禁用、板载点按快捷键、单次/开关软件宏|任意板载宏、按住循环；左键保护；其他 Pulsar 型号|
|Razer Viper V3 Pro 1532:00C0 / 00C1|右、后退、前进|已核对的鼠标动作、禁用、恢复默认|中键、键盘、宏、Hypershift|
|同上|滚轮上、滚轮下、底部 DPI 键|启用原动作或禁用|这些控制之间任意交叉映射|

以上新增只有公开协议/抓包交叉核对、原生驱动与模拟测试，没有对应实机。Pulsar 原来的未知快捷键、宏记录锁定保留；Viper 的未识别原动作和非零未知字段同样锁定。写入目标单独校验，失败尝试恢复原记录。

原有迈从 A7、ATK/VXE/VGN、SinoWealth 白名单、Naga、Corsair、SteelSeries、HyperX 及五款其他有线型号的按键路径保留。各自宏模式、按键数量和读回能力见 [完整兼容表](MOUSE-BRAND-SUPPORT.md)。原有罗技功能也保留，不宣称已转换全部 G HUB 按键或宏。

## 后续需要逐项完成

更多鼠标的额外侧键、狙击键、DPI 键、拨动键、滚轮倾斜、模式层可能采用不同命令和事件，不一定向 Windows 发送标准按键。缺少协议或对应硬件时无法诚实保证“所有鼠标的所有按键都兼容”。每项新能力应包含报文证据、模型校验、实际执行与读回、按键恢复、界面以及保存方案重新执行的测试。

Pulsar 来源：[MIT 参考工具](https://github.com/andrewrabert/python-pulsar-mouse-tool/tree/587f3a996a32aaea0f40e671756808f2f37e7fe2)、[OpenMouse 共享记录](https://github.com/OpenMouse-Project/mouse-protocol/blob/23f5bed75d148cabeed19c74838deb31d3f60d02/src/teevolution/index.ts)。Viper 控制来源：[固定提交的独立抓包测试](https://github.com/OpenMouse-Project/mouse-protocol/blob/23f5bed75d148cabeed19c74838deb31d3f60d02/src/drivers/razer/protocol.test.ts)。仅独立转录协议事实，没有运行厂商配置器或导入全套运行时。

Pulsar 快捷键最多一个普通键及四个修饰键。软件宏必须验证目标鼠标和键盘输入接口的序列号且无重复歧义；F13–F24 点按触发也可能传递给其他程序，关闭软件会尝试恢复原绑定。原厂未知宏保留，不宣称可以导入原厂宏。多只同型号时，配置中心须选择一个设备；自动切换跳过歧义目标。协议依据另见 [Pulsar 说明](PULSAR-ADAPTER.md)。

Basilisk 的原始板载槽及恢复归属单独核对；完整范围、固定协议来源和未实机状态见 [BASILISK-ADAPTER.md](BASILISK-ADAPTER.md)。
