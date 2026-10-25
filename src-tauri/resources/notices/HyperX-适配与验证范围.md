# HyperX Pulsefire Haste 实验适配

仅 HP USB ID `03F0:048E`（有线）和 `03F0:028E`（无线），USB 配置接口 2、Usage Page FF00。不包含 Haste 2/3、Core、Mini、其他 PID 或旧 Kingston ID。没有 HyperX 实机；通过生产 Rust 驱动、内存 HID、公开 USB 抓包和浏览器 UI 验证。

已实现 200–16000 DPI、100 DPI 步进、1–5 档、125/250/500/1000 Hz；保护左键，支持已知基础鼠标动作、禁用、DPI 循环及键盘动作。修饰键快捷键以键盘点按宏实现。原生板载宏只播放一次，最多 80 个键盘状态、同时六个普通键；必须配对释放，单次延时最多 10 秒、总延时最多 60 秒。不编码鼠标事件、文本或等待释放，不启用未验证的循环命令。

滚轮灯区支持常亮/关闭及亮度。发送六页颜色、模式及保存命令，不能读回原厂灯效；打开设备时不伪造当前灯色。颜色仅保留本次会话值，跨断电持久性未作实机保证。屏幕/音乐及电脑光同步仍由既有独立灯光引擎按其设备能力提供。

“HyperX 板载配置导入”读取并预览 DPI、回报率及能识别的按键，再保存到配置中心；不会写鼠标，不解析 NGENUITY 私有文件。原厂宏内容不可读回；本程序在当前会话创建的宏可随方案保存。已有宏绑定或无法识别的动作不被覆盖，任何其他按键引用目标宏槽时也拒绝写入。固件无法导出未绑定的旧宏数据；在未被引用的槽创建新宏可能替换该槽的旧内容。宏报文仅检查传输完整及绑定读回，不能证明宏内容已在硬件执行。重新连接后原厂宏槽保持未知/锁定。

DPI、回报率和已知按键写入后读回；不一致或失败时尝试恢复该项原值，并读回确认。不能保证多项设置整体原子执行，错误会指出此前项目可能已生效。查询硬件信息并复核 VID/PID 后才写设置；错误身份、短报文、短写入、无关响应、越界参数、左键修改均拒绝。

协议参考 [Evan Razzaque 的 MIT libratbag 驱动](https://github.com/evan-razzaque/libratbag/tree/eceface88e0ce228eb4a335f10689e9f6677b648)。独立核对 [原作者协议与 USB 抓包](https://github.com/evan-razzaque/open-pulsefire-haste/tree/294f7f0edf6d8f4e1db668d08a85828c452c9f69)：回报率长度采用抓包的 1，按键输出与读回第三字段按协议分别处理。64 字节 USB 载荷在 hidapi 输出前增加 unnumbered Report ID 0，API 写入为 65 字节；Windows 输入由 hidapi 去除零 ID。公开抓包只保存提取后的报文与来源哈希，见 `src-tauri/test-data/hyperx-public-captures.json`，未捆绑 USB 抓包工具。

协议作者的 Haste SVG 保留 GPL-3.0 许可，SVG 即可编辑源码；无线款照片来自 HyperX 官方商品页。两者只是外观资料，不能扩大协议支持范围。

Haste SVG 仅去除独立标注引线层并收紧画布，主体路径未改；原始 SVG 另存 docs/upstream/HyperX-Pulsefire-Haste-original.svg，保留原作者与 GPL-3.0 许可。
