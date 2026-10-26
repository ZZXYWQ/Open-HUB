<div align="center">


<img src="public/open-hub.png" width="96" height="96" alt="Open HUB">

# Open HUB · 实验性版本

**基于 [abidr/open-ghub（Open GHub）](https://github.com/abidr/open-ghub) 修改的多品牌鼠标控制实验项目。**

**[⬇ 下载 Windows x64 EXE 安装包 · 0.3.7 实验版（约 37 MiB）](https://github.com/ZZXYWQ/Open-HUB/releases/download/v0.3.7-experimental.1/Open-HUB-0.3.7-x64-setup.exe)**

[版本说明与全部附件](https://github.com/ZZXYWQ/Open-HUB/releases/tag/v0.3.7-experimental.1) ·
[对应源码 ZIP](https://github.com/ZZXYWQ/Open-HUB/releases/download/v0.3.7-experimental.1/Open-HUB-0.3.7-source.zip) ·
[完整适配范围](MOUSE-BRAND-SUPPORT.md) · [验证记录](RELEASE-VALIDATION.md)

</div>

正在开发 [100 款鼠标完整适配目标](docs/100-MODEL-ACCEPTANCE.md)。当前下载仍为上方 **0.3.7 实验版**，不包含后续开发增量；达到完整验收目标后再生成新的安装包。

开发分支已接入[罗技原生板载按键](docs/LOGITECH-ONBOARD-BINDINGS.md)、[性能与 X11 方案](docs/development/2026-10-26-performance-x11.md)、[Sensei Ten 五档八键与双区灯效](docs/SENSEI-TEN.md)、[Rival 600 七键八灯区与抬升](docs/RIVAL600.md)。[DeathAdder V3 Pro](docs/RAZER-DAV3PRO-RESEARCH.md) 已接通五档 XY DPI、回报率、省电、已知十六条双层按键、Hypershift、传感器校准与独立 XY 狙击 DPI；支持的 Synapse 4 子集可以预览、编辑、保存及回放验证，未知功能仍保留并阻止不完整导入。

最近增量补充[罗技原生呼吸波形](docs/LOGITECH-NATIVE-LIGHTING.md)、[电脑与底座独立灯光](docs/LIGHTING-INDEPENDENT-OUTPUTS.md)及[迈从界面离开目标后的异步隔离](docs/development/2026-10-26-mchose-target-lifetime-validation.json)。[开发验证记录](docs/development/2026-10-26-shiftxy-waveform-validation.json)区分原厂静态证据、协议模拟、实际界面测试与缺少的实机覆盖；软件宏来源及完整型号验收继续推进。

迈从后续修复保存其他设置时丢失草稿、切换板载组时旧操作重新选取目标组的问题；各板载组与宏槽保留独立草稿，写入始终使用原先核对的组。[611 项主程序测试及 20 组相关 UI 回归通过](docs/development/2026-10-27-mchose-drafts-board-validation.json)，新增功能仍未计作实机完整验收。

> [!WARNING]
> **这仅是一个实验性版本，不是稳定版。** 多数新增鼠标只有生产驱动与模拟设备测试，没有对应实机验证，不能认定整个品牌兼容。连接方式、USB ID、固件和功能边界均会影响支持情况。应用前请先保留原配置，只对明确列出的型号进行测试。

## 来源与改动

本项目修改自 **Abidur Rahman 的 [Open GHub](https://github.com/abidr/open-ghub)**，保留上游提交历史、作者信息与 GPL-3.0-or-later 许可。修改基线为 [`2853a6251a31a376a0f7287c1c846bd9bff97198`](https://github.com/abidr/open-ghub/commit/2853a6251a31a376a0f7287c1c846bd9bff97198)。[上游原始 README](docs/upstream/Open-GHub-README.md) 仅作来源记录，其中的下载、签名和测试声明属于上游。

本分支改名为 **Open HUB**，增加迈从及部分其他品牌型号的设备适配、按品牌分类的配置导入、对应功能界面、板载/软件宏、灯光同步、新图标、配色、透明度与小型 JSON 皮肤。保留原有罗技 HID++ 路径，按品牌、型号和设备身份隔离配置。

本项目与各硬件厂商及上游作者均无官方隶属或背书关系。

本轮新增 Basilisk V3 Pro 有线/2.4G 的 12 个独立控制、键盘快捷键、单次/开关软件宏、DPI、灯效和对应界面，仍为 23 个品牌的限定范围；[按键支持与限制](BUTTON-COMPATIBILITY.md)。跨品牌和 SinoWealth 方案在多只同型号设备连接时必须选择一个目标，自动切换遇到歧义会跳过。保留逐设备按键配置，不加入全局重映射。

## 哪些设备适配到什么程度

**当前涉及 23 个品牌的部分型号，不是全品牌兼容。** 上游原有测试不计作本分支新增实机验证。

|品牌|明确型号 / 范围|已实现的主要范围|本分支验证|
|---|---|---|---|
|罗技 Logitech|原有 HID++ 2.0 路径，按能力显示|保留原功能；G HUB 导入 DPI、回报率和应用关联，不转换官方按键、宏、灯效|保留上游实现、自动化隔离检查；未做罗技实机回归|
|迈从 MCHOSE|A7 V3 Ultra+、MagDock|DPI、性能、按键、板载宏、三组板载配置；底座原厂效果与软件同步；M HUB 板载名称/引用导入|**使用这款鼠标和底座实机验证**；不代表全部功能/固件均已认证|
|雷蛇 Razer|Naga V2 HyperSpeed；DeathAdder V2；Viper Ultimate / V2 Pro / V3 Pro；Basilisk V3 Pro|Naga 的 DPI、回报率、支持的按键/软件宏及有限 Synapse 转换；Basilisk 的 12 控制及独立软件宏、DPI/RGB；其余型号能力不同|生产驱动 + 模拟，未实机验证|
|海盗船 Corsair|M65 PRO RGB，1B1C:1B2E|DPI、回报率、支持的按键/软件宏、有限 iCUE 转换；部分状态来自缓存，重连需核对|生产驱动 + 模拟，未实机验证|
|赛睿 SteelSeries|Rival 310，1038:1720|两组 CPI、回报率、支持的按键/软件宏、有限 GG 数据库转换；部分状态来自写入缓存|生产驱动 + 模拟，未实机验证|
|HyperX|Pulsefire Haste / Haste Wireless，03F0:048E / 028E|DPI、回报率、已知按键、单次键盘板载宏、常亮/关闭灯光和板载读取；原厂宏/灯效不可读回|生产驱动 + 模拟与公开抓包核对，未实机验证|
|Pulsar|X2 V2 Mini，3554:F507 / F508|四档 DPI、回报率、常亮/关闭指示灯、基础按键、DPI 动作、板载点按快捷键、单次/开关软件宏；软件宏须核验同一鼠标的序列号，任意板载宏未启用|生产驱动 + 模拟，未实机验证|
|华硕 / ASUS|ROG Strix Impact 0B05:1847|固定两档 DPI、回报率、实际四个按键、单键映射和四种 Logo 灯效；任意宏未启用|生产驱动 + 模拟，未实机验证|
|Roccat|Kone Pure 1E7D:2DC2|五档 XY DPI、回报率、已知按键、单次键盘板载宏；灯效暂未启用|生产驱动 + 模拟，未实机验证|
|芝奇 / G.Skill|MX-780 28DA:3101|五档 XY DPI、500/1000 Hz、按键、快捷键和单次键盘板载宏；保留原灯效字段|生产驱动 + 模拟，未实机验证|
|Etekcity|Scroll Alpha 1EA7:4011|六档 XY DPI、回报率、按键和最多 50 事件的单次键盘宏；间隔由固件控制，灯效暂未启用|生产驱动 + 模拟，未实机验证|
|Mars Gaming|MM4 04D9:FA58|五档 XY DPI、回报率、按键/快捷键、关闭/常亮/呼吸；任意宏未启用|生产驱动 + 模拟，未实机验证|
|ATK|A9 Plus NK：373B:1115 / 老 NearLink 10C9；X1 Pro Max USB：373B:1017|八档 XY DPI、对应回报率、基础按键、键盘点按、板载宏、只读板载导入；X1 指示灯关闭/常亮|生产驱动 + 模拟，**没有 ATK 实机**|
|蜻蜓 VXE|R1、R1 Pro、MAD R、MAD R+ 的精确 USB/CID/MID|DPI、回报率、按键、键盘点按、板载宏、读取导入和型号外观；R1 系列 DPI 指示灯|生产驱动 + 模拟，未实机验证|
|VGN|蜻蜓 F1 Pro / F1 Pro Max 的精确 USB/CID/MID|上述 COMPX 功能及当前 DPI 指示灯；两品牌分别列出|生产驱动 + 模拟，未实机验证|
|Glorious|Model O/O- V103、Model D V102、Model O Eternal 1009、老 Model O V102|下述 SinoWealth 范围|生产驱动 + 模拟，未实机验证|
|G-Wolves|Hati HT-M Wired V161|下述 SinoWealth 范围|生产驱动 + 模拟，未实机验证|
|DreamMachines|DM5 Blink 3106|下述 SinoWealth 范围|生产驱动 + 模拟，未实机验证|
|Genesis|Xenon 770 3110|下述 SinoWealth 范围|生产驱动 + 模拟，未实机验证|
|Machenike 机械师|M620 V127|下述 SinoWealth 范围|生产驱动 + 模拟，未实机验证|
|Mad Dog|GM905 V287|下述 SinoWealth 范围|生产驱动 + 模拟，未实机验证|
|T-Dagger|Imperial T-TGM310 2642|下述 SinoWealth 范围|生产驱动 + 模拟，未实机验证|
|Marvo|Scorpion G961 9677|下述 SinoWealth 范围|生产驱动 + 模拟，未实机验证|

SinoWealth 范围：仅表中型号/固件的 DPI、回报率、按键、程序记录的板载宏和板载读取。原厂宏内容不可读回，灯光另取决于实际接口。以上 8 个品牌为 11 条型号/固件记录，不能把固件和有线/无线 PID 重复计作新鼠标。

[Basilisk V3 Pro 精确范围](BASILISK-ADAPTER.md)：仅 00AA/00AB、普通层、当前板载槽；原厂宏/Hypershift 保留，尚未实机。Synapse 私有导入仍仅支持 Naga 的已实现格式。

雷蛇型号之间也不同：DeathAdder V2 和 Viper Ultimate 未启用按键/宏；Viper V2 Pro 无 RGB、未启用按键/宏；Viper V3 Pro 支持指定物理键的鼠标动作映射，以及滚轮方向和底部 DPI 键的原动作恢复/禁用，无键盘动作/宏/RGB。Naga 的转换能力不适用于这些 Viper。新增六款 VXE/VGN 的 MID 白名单及边界见 [COMPX 适配说明](COMPX-ADAPTER.md)。新增 [HyperX 范围](HYPERX-ADAPTER.md) 和 [Pulsar 范围](PULSAR-ADAPTER.md)；精确 USB ID、DPI、回报率和灯效边界见 [完整适配表](MOUSE-BRAND-SUPPORT.md)、[ATK 说明](ATK-ADAPTER.md)、[SinoWealth 说明](SINO-ADAPTER.md)。

**尚未接入：** AULA 狼蛛 SC620 / SC580 及未明确列出的其他型号。研究表或灯光引擎中出现品牌名称不等于完成 DPI、按键、宏和官方配置适配。ATK X1 无线接收器、A9 2.0、新 BITMOUSE/FF05 协议也不在本版范围。

## 配置导入与灯光边界

导入菜单按品牌列出，先预览再保存，不自动应用鼠标。G HUB 仅导入已支持字段；M HUB 仅导入本地三组板载名称与引用，不迁移云端或未写入鼠标的完整方案。Synapse / iCUE / GG 只转换实现过的格式、型号和动作，不保证任意官方文件可用。ASUS、Roccat、G.Skill、Etekcity、Mars Gaming、HyperX、Pulsar、ATK、VXE、VGN 和上述 SinoWealth 型号通过设备板载读取导入，**不是官方软件文件解析器**。

灯光界面按实际能力识别品牌和鼠标，原厂效果按型号提供。电脑 RGB 同步使用按需启动的 OpenRGB 1.1，只有匹配设备、Direct 模式和灯珠均满足才允许同步。RGB 兼容不代表 DPI、按键、宏兼容，不自动安装内核驱动。音乐使用系统播放电平，屏幕取色读取主显示器少量像素，均在本机处理。

## 外观

设备中心、原生设置、离线方案编辑和已识别的鼠标灯光界面按型号显示产品图或对应轮廓。28 张 WebP + 25 张 SVG 共约 761 KiB，未知型号不套用其他鼠标图片；配色选择仅影响展示。详见 [外观资源与来源](docs/DEVICE-ARTWORK.md)。

齿轮 → **外观与设置**：4 组预设、主题/背景/面板颜色、透明或半透明、圆角和小型皮肤导入/导出。皮肤是最多 8 KiB 的严格 JSON，不加载任意脚本、图片包或 CSS。新图标为抽象 Z 字轨迹；**图标六色切换尚未实现**。见 [外观说明](APPEARANCE.md)。

## 测试与实验性质

当前覆盖 **23 个品牌的限定型号**；本次补充已有型号的按键能力，品牌数不变。上一批五款有线鼠标的范围见 [五品牌说明](FIVE-BRAND-ADAPTERS.md)。

本次 0.3.7 的检查：

- **321 项主程序原生测试通过**（316 项默认回归 + 5 项提供输入的交叉测试）。覆盖 Pulsar 快捷键完整校验、每阶段短写恢复、原厂未知记录保留、两只同型号鼠标的宏隔离、切组恢复保护、配置单目标校验和界面保存方案重新执行。
- **21 组浏览器界面回归通过**，浏览器模拟 Tauri；生产驱动使用内存 HID。61 个目录身份的图片加载检查不等于新增 61 款硬件支持。
- 公开 HyperX 原始抓包独立核对回报率、按键输出及宏布局，原始文件哈希和提取报文保留；仍不能替代 Windows 实机验证。
- Windows 生产 EXE、安装包载荷和签名检查见 [验证摘要](docs/releases/0.3.7-validation.json)。应用内更新继续由用户选择下载与安装；开发者本机按用户明确要求在每次发布后升级，个人配置备份记录不公开。公开 EXE 已匿名完整下载核对哈希，生产程序验证签名、篡改拒绝和版本绑定均通过，见 [发布后验收](docs/releases/0.3.7-public-verification.json)。
- 历史共享驱动检查为 289 个单元测试 + 1 个文档测试；Synapse 解析器为 148 个单元测试 + 3 个集成测试，作为历史回归证据列出。

**未覆盖：** 其他鼠标实机、全新 Windows / 新用户、缺失 WebView2 的联网补装、所有固件变体、桌面透明效果的完整人工视觉验证。测试通过不构成全品牌兼容或稳定性保证。见 [公开验证摘要](docs/releases/0.3.7-validation.json) 和 [测试边界](RELEASE-VALIDATION.md)。

## 安装与校验

1. 点击主页顶部 **EXE 下载**，或进入 [实验版 Release](https://github.com/ZZXYWQ/Open-HUB/releases/tag/v0.3.7-experimental.1)。
2. 运行 `Open-HUB-0.3.7-x64-setup.exe`，按当前用户安装到 `%LOCALAPPDATA%\Programs\Open HUB`。
3. 从开始菜单打开 Open HUB。测试设备前先退出对应官方控制中心，避免反复覆盖设置。

Windows 10 / 11 x64，需要 Microsoft Edge WebView2。约 37 MiB 的常规包复用系统 WebView2，缺少时联网补装，不携带完整浏览器运行库。**本实验 EXE 未代码签名**，不继承上游 SignPath 认证；自 0.3.2 起提供本仓库 GitHub 更新：启动后自动查询，可关闭；下载和安装分两次由用户选择，安装前验证版本绑定签名。0.3.1 旧客户端首次需手动安装新版。见 [更新说明](docs/UPDATES.md)。

EXE 的 SHA-256：

```text
1085bd7e9d3963207d0309acf2c8c47ad1768cbe51edc4871cc11b5dd9c7c4c5
```

Release 同时提供对应源码 ZIP、`SHA256SUMS.txt` 与公开验证摘要。源码包含第三方许可与 OpenRGB 对应源码存档，不上传个人配置、设备备份、账号信息或厂商驱动研究文件。

## 开发与许可

构建和测试见 [BUILDING.md](BUILDING.md)，体积与安装机制见 [PACKAGING.md](PACKAGING.md)。

Open GHub / 本衍生项目采用 [GPL-3.0-or-later](LICENSE)。原作者和第三方署名保留，组件包括 MIT 许可的 Gearwise、Sidearm Synapse parser、libratbag 协议参考，以及 GPL-2.0-or-later 的 OpenRGB。见 [来源与组件](NOTICE.md) 及随包许可。厂商名称和设备图片权利属于原权利人，仅用于型号识别与说明。
