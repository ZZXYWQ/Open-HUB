# Pulsar X2 V2 Mini 实验适配

本版只接 X2 V2 Mini 的 `3554:F507` USB 与 `3554:F508` 1K 接收器，共一个型号、两种连接。USB 配置接口 1，17 字节 Report 08；不是整个 Pulsar 品牌，也不是 X2 Medium/X2H/Xlite/CrazyLight 或 4K 接收器。

复用现有 COMPX 报文、校验、读回及逐行恢复，没有新增驱动运行时。50–26000 DPI、50 步进、最多四档、统一 XY；125/250/500/1000 Hz；当前档位指示灯关闭/常亮。呼吸、亮度和其他灯区未开放写入。0.3.5 起，右键、中键、后退和前进可独立写入五种鼠标动作、DPI 循环/升降及禁用，写后回读。左键保留。键盘和板载宏格式未核对，UI、方案编译及驱动层继续拒绝。

“Pulsar 板载配置导入”预览并保存 DPI、回报率、已知基础按键与能读回的常亮/关闭指示灯，不解析 Pulsar Fusion 文件。未知动作或其他灯效不伪装默认值。打开时只查询并读取；在线状态与当前配置组被固定，组发生改变后拒绝继续写入，须重新扫描设备。未宣称取得过本型号 CID/MID；不套用 ATK 的型号编号，也不删除已有 ATK/VXE/VGN CID/MID 检查。

依据 [Andrew Rabert 的 MIT 协议工具](https://github.com/andrewrabert/python-pulsar-mouse-tool/tree/587f3a996a32aaea0f40e671756808f2f37e7fe2)。仅参考静态协议事实，不运行其 USB reset、恢复出厂或配置切换代码。原工具存在部分未完成的操作，本实现另行校验校验和、DPI 上限、当前组与读回；没有加入未验证命令。

实物图来自 [Pulsar 官方 X2 V2 Mini 商品页](https://www.pulsar.gg/products/x2v2-mini-gaming-mouse)，仅展示外观。当前只有模拟验证，没有 Pulsar 实机，需继续验证 Windows USB 描述符、固件差异及无线唤醒。

按键格式另与 [OpenMouse 固定提交的共享按键记录](https://github.com/OpenMouse-Project/mouse-protocol/blob/23f5bed75d148cabeed19c74838deb31d3f60d02/src/teevolution/index.ts) 交叉核对。未知原厂快捷键/宏记录锁定保留，不套用 ATK 的板载宏载荷。模拟覆盖两种连接、四个可改物理键的全部已知动作、独立校验向量、回读失败逐行恢复、未知原动作和配置组变化。
