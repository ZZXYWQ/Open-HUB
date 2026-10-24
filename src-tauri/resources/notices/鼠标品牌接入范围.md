# Open HUB 鼠标接入范围

当前版本：2026-10-24，Open HUB 0.3.1 加入 ATK 首批适配，具体安装升级证据见输出 verification.json。除迈从 A7 V3 Ultra+ 与 MagDock 外，新增型号均以生产驱动和合成设备测试，尚未实机验证。

## 当前品牌与型号

|品牌|型号或范围|配置导入与控制范围|
|---|---|---|
|罗技 / Logitech|原有 HID++ 识别目录，按探测能力显示|保留既有控制；G HUB 导入 DPI、回报率和应用关联，暂不转换官方按键、宏和灯效|
|迈从 / MCHOSE|A7 V3 Ultra+、MagDock 底座|DPI、性能、按键、宏、板载组；底座原厂灯效与同步。M HUB 导入三组板载名称与引用，不复制设备已有参数|
|雷蛇 / Razer|Naga V2 HyperSpeed、DeathAdder V2、Viper Ultimate、Viper V2 Pro、Viper V3 Pro|能力因型号而异，见下文；Synapse 可执行转换目前仅支持 Naga 的已实现格式|
|海盗船 / Corsair|M65 PRO RGB（1B1C:1B2E）|DPI、回报率、支持的按键/软件宏、iCUE 转换；部分状态来自缓存，回报率重连需核对续接|
|赛睿 / SteelSeries|Rival 310（1038:1720）|两组 CPI、回报率、支持的按键/软件宏、GG 数据库转换；部分状态来自写入缓存|
|ATK|A9 Plus NK（373B:1115 / 10C9）、X1 Pro Max USB（373B:1017）|八档 XY DPI、对应回报率、基础按键/键盘点按/板载宏、板载配置读取导入；X1 指示灯关闭/常亮。仅模拟验证，完整边界见 ATK-ADAPTER.md|

SinoWealth 通道的以下指定型号/固件支持 DPI、回报率、按键、程序记录的板载宏和板载配置读取。原厂宏内容不可读回；灯光取决于独立的实际灯光接口。此表从当前原生型号目录生成：

|品牌|指定型号|固件|
|---|---|---|
|Glorious|Glorious Model O/O-|V103|
|Glorious|Glorious Model D|V102|
|Glorious|Glorious Model O Eternal|1009|
|Glorious|Glorious Model O (old firmware)|V102|
|G-Wolves|G-Wolves Hati HT-M Wired|V161|
|DreamMachines|DreamMachines DM5 Blink|3106|
|Genesis|Genesis Xenon 770|3110|
|Machenike|Machenike M620|V127|
|Mad Dog|Mad Dog GM905|V287|
|T-Dagger|T-Dagger Imperial T-TGM310|2642|
|Marvo|Marvo Scorpion G961|9677|

以上涉及 14 个品牌的部分型号，不是全品牌兼容。SinoWealth 的 8 个品牌共有 11 条型号/固件记录，不能把固件或有线/无线 PID 重复计为新鼠标。

雷蛇能力按型号区分：

- Naga V2 HyperSpeed：当前支持的 DPI、回报率、按键、软件宏与有限制的 Synapse 转换。
- DeathAdder V2：DPI、回报率和已实现灯效；按键及宏未启用。
- Viper Ultimate（007A/007B）：五档 DPI、回报率、Logo 五种原生效果；按键及宏未启用。
- Viper V2 Pro（00A5/00A6）：五档 DPI、原配接收器 125/500/1000 Hz；无 RGB，按键及宏未启用。
- Viper V3 Pro（00C0/00C1）：五档 DPI，有线最高 1000 Hz、无线最高 8000 Hz；右键/前进/后退支持已验证的鼠标动作映射，左键受保护。键盘动作、宏及其他物理键未启用，无 RGB。
- Viper 可创建、保存和模拟 Open HUB 方案，但 Synapse 转换尚未覆盖。六个连接 PID 对应三款鼠标。

## 配置中心与灯光

导入入口分别标为罗技 G HUB、迈从 M HUB、雷蛇 Synapse、海盗船 iCUE、赛睿 GG，注明各自支持型号与范围。其他已适配品牌分别列入板载读取区；不伪装成官方软件文件解析器。进入品牌后仅显示其精确匹配的鼠标，返回结果的品牌/型号不符会被拦截。导入不自动应用设备。顶部仅保留一个“导入配置”按钮，点开后平铺各品牌；Open HUB 自己的配置包在同一选择窗底部以“恢复 Open HUB 备份”进入。

完整会话方案保留原始按键、宏、播放方式和输入来源，区别于硬件读回与驱动缓存。停止选中鼠标不停止其他鼠标。重启恢复及 M65 回报率续接要求核对明确目标，不自动向同品牌替代设备写入。

首页不自动启动 RGB 扫描；点击扫描或进入灯光同步后才检测。灯光页根据实际设备能力识别品牌与鼠标，只有鼠标类型、Direct 模式和灯珠都满足才允许同步。新增设备不自动启动效果或加入运行中的同步。有灯光接口不能证明 DPI、按键和宏也已适配。

## 验证与安装状态

0.3.1 原生回归 239 项通过，包含 19 项 ATK 专项；全部 15 组浏览器回归通过。

以下为 0.3.0 的历史验证记录：原生回归 217 项通过，供应驱动工作区 289 个单元测试及 1 个文档测试通过。最新前端构建和全部 13 组浏览器界面回归通过，品牌导入新增 20 项检查。界面使用模拟 Tauri，不能替代实机验证。完整方案回显、缓存说明、重启恢复及 M65 续接均已有模拟测试。新增三条跨层回归从实际 Synapse 4/iCUE/SQLite 文件入口导入并保存到磁盘，重新读取后通过生产驱动和模拟 HID 写入，再从对应输入来源触发完整宏输出；文件是明确标注的合成测试样本，不是新增实机记录。Synapse 解析器 148 个单元测试及 3 个集成测试通过，包含公开真实导出样本。本机已完成 0.2.3 → 0.3.0 静默升级：35 个载荷文件逐字节一致，292 个原配置与备份文件不变，新程序已启动。真实 NSIS 独立样本覆盖嵌套/中文路径、同名保护、重复迁移、目标目录失败、复制共享冲突及无来源；同版本覆盖安装及无程序目录/卸载注册项后的重新安装均通过，原有配置继续保留。此验证复用了当前 Windows、用户配置与系统 WebView2；未验证全新系统、新用户配置或缺少 WebView2 时的联网补装。

常规安装程序复用系统 WebView2，缺少时联网补装，使用 LZMA 压缩；完整离线运行库为可选构建。共享 RGB 组件及对应源码/许可保留，见 PACKAGING.md。官方软件研究文件、开发工具和历史安装包不进入应用资源。

AULA SC620/SC580 目前只有官方驱动静态研究，尚无可交付适配。ROG、HyperX、VGN 等也不能因为调查表中列有名称就宣称已接入。下面是调查线索，区别于当前支持清单。

## 官方软件调查

|品牌|软件调查线索|官方入口|本次核对|
|---|---|---|---|
|AJAZZ|各型号驱动|[AJAZZ](https://ajazzbrand.com/pages/software)|当前无法核实官网页面|
|Alienware|Command Center|[Alienware](https://www.dell.com/support/home/)|当前无法核实官网页面|
|AOC|G-Menu|[AOC](https://aoc.com/)|官网可访问；仍需逐型号核对|
|ASUS ROG|Armoury Crate / Gear Link|[ASUS ROG](https://rog.asus.com/content/armoury-crate/)|官网可访问；仍需逐型号核对|
|ATK|ATK HUB|[ATK](https://hub.atk.pro/)|已核对官方固定版本脚本；本版只接入上方两款鼠标及三种连接|
|Attack Shark|驱动 / 网页驱动|[Attack Shark](https://attackshark.com/pages/driver-download)|当前无法核实官网页面|
|Bloody / A4Tech|Bloody|[Bloody / A4Tech](https://www.bloody.com/en/download.php)|当前无法核实官网页面|
|Cooler Master|MasterPlus+|[Cooler Master](https://masterplus.coolermaster.com/)|官网可访问；仍需逐型号核对|
|Corsair|iCUE|[Corsair](https://www.corsair.com/us/en/s/icue)|官网可访问；仍需逐型号核对|
|COUGAR|UIX|[COUGAR](https://cougargaming.com/downloads/)|官网可访问；仍需逐型号核对|
|Dareu|驱动 / 网页驱动|[Dareu](https://www.dareu.com/)|当前无法核实官网页面|
|Darmoshark|驱动 / 网页驱动|[Darmoshark](https://darmoshark.com/)|当前无法核实官网页面|
|Delux|各型号驱动|[Delux](https://www.deluxworld.com/)|官网可访问；仍需逐型号核对|
|Endgame Gear|配置工具|[Endgame Gear](https://www.endgamegear.com/en-us/downloads)|当前无法核实官网页面|
|EVGA|UNLEASH RGB|[EVGA](https://www.evga.com/unleash/)|官网可访问；仍需逐型号核对|
|Fantech|各型号软件|[Fantech](https://fantechworld.com/pages/download)|当前无法核实官网页面|
|Genesis|各型号软件|[Genesis](https://genesis-zone.com/support)|当前无法核实官网页面|
|GIGABYTE / AORUS|Control Center / Engine|[GIGABYTE / AORUS](https://www.gigabyte.com/Consumer/Software/GIGABYTE-Control-Center/global/)|当前无法核实官网页面|
|Glorious|CORE|[Glorious](https://www.gloriousgaming.com/pages/software)|当前无法核实官网页面|
|HyperX|NGENUITY|[HyperX](https://hyperx.com/pages/ngenuity)|当前无法核实官网页面|
|Keychron|Launcher|[Keychron](https://launcher.keychron.com/)|官网可访问；仍需逐型号核对|
|LAMZU|软件 / 网页驱动|[LAMZU](https://lamzu.com/pages/download)|当前无法核实官网页面|
|Lenovo Legion|Accessories Central|[Lenovo Legion](https://support.lenovo.com/)|当前无法核实官网页面|
|Logitech|G HUB|[Logitech](https://www.logitechg.com/innovation/g-hub.html)|官网可访问；仍需逐型号核对|
|MCHOSE|M HUB|[MCHOSE](https://www.mchose.com.cn/)|官网可访问；仍需逐型号核对|
|MSI|MSI Center|[MSI](https://www.msi.com/Landing/MSI-Center)|当前无法核实官网页面|
|Ninjutso|Sora 配置工具|[Ninjutso](https://www.ninjutso.com/)|当前无法核实官网页面|
|NZXT|CAM|[NZXT](https://nzxt.com/software/cam)|当前无法核实官网页面|
|Patriot Viper|Viper 软件|[Patriot Viper](https://viper.patriotmemory.com/pages/downloads)|官网可访问；仍需逐型号核对|
|Pulsar|Fusion / 各型号驱动|[Pulsar](https://www.pulsar.gg/pages/download)|当前无法核实官网页面|
|RAPOO|各型号驱动|[RAPOO](https://www.rapoo.com/)|官网可访问；仍需逐型号核对|
|Razer|Synapse|[Razer](https://www.razer.com/synapse-4)|官网可访问；仍需逐型号核对|
|Redragon|各型号驱动|[Redragon](https://www.redragonzone.com/pages/download)|当前无法核实官网页面|
|ROCCAT / Turtle Beach|Swarm|[ROCCAT / Turtle Beach](https://support.turtlebeach.com/s/downloads)|官网可访问；仍需逐型号核对|
|Sharkoon|各型号软件|[Sharkoon](https://en.sharkoon.com/)|官网可访问；仍需逐型号核对|
|SteelSeries|GG / Engine|[SteelSeries](https://steelseries.com/gg/engine)|官网可访问；仍需逐型号核对|
|Thermaltake|iTAKE|[Thermaltake](https://www.thermaltake.com/itake)|当前无法核实官网页面|
|Trust|各型号软件|[Trust](https://www.trust.com/en/support)|官网可访问；仍需逐型号核对|
|VGN|V HUB|[VGN](https://www.vgnclub.com/)|当前无法核实官网页面|
|VXE|V HUB / 网页驱动|[VXE](https://www.vxe.com/)|官网可访问；仍需逐型号核对|
|WLmouse|网页驱动|[WLmouse](https://www.wlmouse.com/)|当前无法核实官网页面|

## 内置鼠标控制器记录

从随包分发的上游 OpenRGB 1.1 源码提取，控制器组不等于型号数，有些控制器覆盖多个品牌/型号，部分不支持 Direct。实际以连接后返回的能力为准。尚未连接的品牌没有完成实机测试。

- [AOC Mouse](https://codeberg.org/OpenRGB/OpenRGB/src/tag/release_1.1/Controllers/AOCMouseController/RGBController_AOCMouse.cpp)：AOC；上游 Direct 标记 :white_check_mark:
- [Areson](https://codeberg.org/OpenRGB/OpenRGB/src/tag/release_1.1/Controllers/AresonController/RGBController_Areson.cpp)：Areson；上游 Direct 标记 :x:
- [Asus Aura Mouse](https://codeberg.org/OpenRGB/OpenRGB/src/tag/release_1.1/Controllers/AsusAuraUSBController/AsusAuraMouseController/RGBController_AsusAuraMouse.cpp)：ASUS；上游 Direct 标记 :white_check_mark:
- [Asus Aura Spatha](https://codeberg.org/OpenRGB/OpenRGB/src/tag/release_1.1/Controllers/AsusAuraUSBController/AsusAuraMouseGen1Controller/RGBController_AsusROGSpatha.cpp)：ASUS；上游 Direct 标记 :white_check_mark:
- [Asus Aura Strix Evolve](https://codeberg.org/OpenRGB/OpenRGB/src/tag/release_1.1/Controllers/AsusAuraUSBController/AsusAuraMouseGen1Controller/RGBController_AsusROGStrixEvolve.cpp)：ASUS；上游 Direct 标记 :white_check_mark:
- [Asus Strix Claw](https://codeberg.org/OpenRGB/OpenRGB/src/tag/release_1.1/Controllers/AsusLegacyUSBController/AsusStrixClawController/RGBController_AsusStrixClaw.cpp)：ASUS；上游 Direct 标记 :x:
- [BloodyMouse](https://codeberg.org/OpenRGB/OpenRGB/src/tag/release_1.1/Controllers/A4TechController/BloodyMouseController/RGBController_BloodyMouse.cpp)：Bloody；上游 Direct 标记 :white_check_mark:
- [Coolermaster Master Mouse](https://codeberg.org/OpenRGB/OpenRGB/src/tag/release_1.1/Controllers/CoolerMasterController/CMMM711Controller/RGBController_CMMM711Controller.cpp)：Cooler Master；上游 Direct 标记 :white_check_mark:
- [Coolermaster Master Mouse](https://codeberg.org/OpenRGB/OpenRGB/src/tag/release_1.1/Controllers/CoolerMasterController/CMMM712Controller/RGBController_CMMM712Controller.cpp)：Cooler Master；上游 Direct 标记 :white_check_mark:
- [Coolermaster Master Mouse](https://codeberg.org/OpenRGB/OpenRGB/src/tag/release_1.1/Controllers/CoolerMasterController/CMMMController/RGBController_CMMMController.cpp)：品牌由设备返回；上游 Direct 标记 :white_check_mark:
- [Corsair Peripheral](https://codeberg.org/OpenRGB/OpenRGB/src/tag/release_1.1/Controllers/CorsairPeripheralController/RGBController_CorsairPeripheral.cpp)：Corsair；上游 Direct 标记 :white_check_mark:
- [CorsairPeripheralController](https://codeberg.org/OpenRGB/OpenRGB/src/tag/release_1.1/Controllers/CorsairPeripheralController/CorsairPeripheralController.cpp)：品牌由设备返回；上游 Direct 标记 以连接后的模式为准
- [CorsairPeripheralV2Devices](https://codeberg.org/OpenRGB/OpenRGB/src/tag/release_1.1/Controllers/CorsairPeripheralV2Controller/CorsairPeripheralV2Devices.cpp)：品牌由设备返回；上游 Direct 标记 以连接后的模式为准
- [Cougar Revenger ST](https://codeberg.org/OpenRGB/OpenRGB/src/tag/release_1.1/Controllers/CougarController/CougarRevengerSTController/RGBController_CougarRevengerST.cpp)：Cougar；上游 Direct 标记 :white_check_mark:
- [EVGA USB X20 Mouse](https://codeberg.org/OpenRGB/OpenRGB/src/tag/release_1.1/Controllers/EVGAUSBController/EVGAMouseController/RGBController_EVGAMouse.cpp)：EVGA；上游 Direct 标记 :white_check_mark:
- [EVGA X12 Gaming Mouse](https://codeberg.org/OpenRGB/OpenRGB/src/tag/release_1.1/Controllers/EVGAUSBController/EVGAX12Controller/RGBController_EVGAX12.cpp)：EVGA；上游 Direct 标记 :white_check_mark:
- [Genesis Xenon 200](https://codeberg.org/OpenRGB/OpenRGB/src/tag/release_1.1/Controllers/SinowealthController/GenesisXenon200Controller/RGBController_GenesisXenon200.cpp)：Genesis；上游 Direct 标记 :x:
- [Gigabyte Aorus mouse](https://codeberg.org/OpenRGB/OpenRGB/src/tag/release_1.1/Controllers/GigabyteAorusMouseController/RGBController_GigabyteAorusMouse.cpp)：Gigabyte；上游 Direct 标记 :white_check_mark:
- [HID LampArray Controllers](https://codeberg.org/OpenRGB/OpenRGB/src/tag/release_1.1/Controllers/HIDLampArrayController/RGBController_HIDLampArray.cpp)：品牌由设备返回；上游 Direct 标记 以连接后的模式为准
- [Holtek A070](https://codeberg.org/OpenRGB/OpenRGB/src/tag/release_1.1/Controllers/HoltekController/HoltekA070Controller/RGBController_HoltekA070.cpp)：Holtek；上游 Direct 标记 :x:
- [Holtek A1FA](https://codeberg.org/OpenRGB/OpenRGB/src/tag/release_1.1/Controllers/HoltekController/HoltekA1FAController/RGBController_HoltekA1FA.cpp)：Holtek；上游 Direct 标记 :x:
- [HyperX Pulsefire Dart](https://codeberg.org/OpenRGB/OpenRGB/src/tag/release_1.1/Controllers/HyperXMouseController/HyperXPulsefireDartController/RGBController_HyperXPulsefireDart.cpp)：HyperX；上游 Direct 标记 :white_check_mark:
- [HyperX Pulsefire FPS](https://codeberg.org/OpenRGB/OpenRGB/src/tag/release_1.1/Controllers/HyperXMouseController/HyperXPulsefireFPSProController/RGBController_HyperXPulsefireFPSPro.cpp)：HyperX；上游 Direct 标记 :white_check_mark:
- [HyperX Pulsefire Haste](https://codeberg.org/OpenRGB/OpenRGB/src/tag/release_1.1/Controllers/HyperXMouseController/HyperXPulsefireHasteController/RGBController_HyperXPulsefireHaste.cpp)：HyperX；上游 Direct 标记 :white_check_mark:
- [HyperX Pulsefire Raid](https://codeberg.org/OpenRGB/OpenRGB/src/tag/release_1.1/Controllers/HyperXMouseController/HyperXPulsefireRaidController/RGBController_HyperXPulsefireRaid.cpp)：HyperX；上游 Direct 标记 :white_check_mark:
- [HyperX Pulsefire Surge](https://codeberg.org/OpenRGB/OpenRGB/src/tag/release_1.1/Controllers/HyperXMouseController/HyperXPulsefireSurgeController/RGBController_HyperXPulsefireSurge.cpp)：HyperX；上游 Direct 标记 :white_check_mark:
- [Instant mouse](https://codeberg.org/OpenRGB/OpenRGB/src/tag/release_1.1/Controllers/InstantMouseController/RGBController_InstantMouse.cpp)：品牌由设备返回；上游 Direct 标记 :white_check_mark:
- [Laview Technology Mice](https://codeberg.org/OpenRGB/OpenRGB/src/tag/release_1.1/Controllers/LaviewTechnologyController/LaviewTechnologyController/RGBController_LaviewTechnology.cpp)：品牌由设备返回；上游 Direct 标记 :x:
- [Lenovo Legion M300](https://codeberg.org/OpenRGB/OpenRGB/src/tag/release_1.1/Controllers/LenovoControllers/LenovoM300Controller/RGBController_LenovoM300.cpp)：Lenovo；上游 Direct 标记 :x:
- [Lexip Mouse](https://codeberg.org/OpenRGB/OpenRGB/src/tag/release_1.1/Controllers/LexipMouseController/RGBController_LexipMouse.cpp)：Lexip；上游 Direct 标记 :white_check_mark:
- [Logitech G203L](https://codeberg.org/OpenRGB/OpenRGB/src/tag/release_1.1/Controllers/LogitechController/LogitechG203LController/RGBController_LogitechG203L.cpp)：Logitech；上游 Direct 标记 :white_check_mark:
- [Logitech G600](https://codeberg.org/OpenRGB/OpenRGB/src/tag/release_1.1/Controllers/LogitechController/LogitechG600Controller/RGBController_LogitechG600.cpp)：Logitech；上游 Direct 标记 :white_check_mark:
- [Logitech HID++ 2.0](https://codeberg.org/OpenRGB/OpenRGB/src/tag/release_1.1/Controllers/LogitechController/LogitechHIDPP20Controller/RGBController_LogitechHIDPP20.cpp)：Logitech；上游 Direct 标记 :white_check_mark:
- [Logitech Lightspeed](https://codeberg.org/OpenRGB/OpenRGB/src/tag/release_1.1/Controllers/LogitechController/LogitechLightspeedController/RGBController_LogitechLightspeed.cpp)：Logitech；上游 Direct 标记 :white_check_mark:
- [Logitech Lightsync Mouse](https://codeberg.org/OpenRGB/OpenRGB/src/tag/release_1.1/Controllers/LogitechController/LogitechGLightsyncController/RGBController_LogitechGLightsync.cpp)：Logitech；上游 Direct 标记 :white_check_mark:
- [Logitech Lightsync Mouse (1 Zone)](https://codeberg.org/OpenRGB/OpenRGB/src/tag/release_1.1/Controllers/LogitechController/LogitechGLightsyncController/RGBController_LogitechGLightsync1zone.cpp)：Logitech；上游 Direct 标记 :white_check_mark:
- [LogitechHIDPP20Controller](https://codeberg.org/OpenRGB/OpenRGB/src/tag/release_1.1/Controllers/LogitechController/LogitechHIDPP20Controller/LogitechHIDPP20Controller.cpp)：品牌由设备返回；上游 Direct 标记 以连接后的模式为准
- [N5312A mouse](https://codeberg.org/OpenRGB/OpenRGB/src/tag/release_1.1/Controllers/N5312AController/RGBController_N5312A.cpp)：Unknown；上游 Direct 标记 :white_check_mark:
- [NZXT Mouse](https://codeberg.org/OpenRGB/OpenRGB/src/tag/release_1.1/Controllers/NZXTMouseController/RGBController_NZXTMouse.cpp)：NZXT；上游 Direct 标记 :white_check_mark:
- [Patriot Viper V550](https://codeberg.org/OpenRGB/OpenRGB/src/tag/release_1.1/Controllers/PatriotViperMouseController/RGBController_PatriotViperMouse.cpp)：Patriot；上游 Direct 标记 :white_check_mark:
- [Razer](https://codeberg.org/OpenRGB/OpenRGB/src/tag/release_1.1/Controllers/RazerController/RazerController/RGBController_Razer.cpp)：Razer；上游 Direct 标记 :white_check_mark:
- [RazerController](https://codeberg.org/OpenRGB/OpenRGB/src/tag/release_1.1/Controllers/RazerController/RazerController/RazerController.cpp)：品牌由设备返回；上游 Direct 标记 以连接后的模式为准
- [RazerDevices](https://codeberg.org/OpenRGB/OpenRGB/src/tag/release_1.1/Controllers/RazerController/RazerDevices.cpp)：品牌由设备返回；上游 Direct 标记 以连接后的模式为准
- [Redragon Mice](https://codeberg.org/OpenRGB/OpenRGB/src/tag/release_1.1/Controllers/RedragonController/RGBController_RedragonMouse.cpp)：Redragon；上游 Direct 标记 :x:
- [Roccat Burst Mouse](https://codeberg.org/OpenRGB/OpenRGB/src/tag/release_1.1/Controllers/RoccatController/RoccatBurstController/RGBController_RoccatBurst.cpp)：Roccat；上游 Direct 标记 :white_check_mark:
- [Roccat Burst Pro Air](https://codeberg.org/OpenRGB/OpenRGB/src/tag/release_1.1/Controllers/RoccatController/RoccatBurstProAirController/RGBController_RoccatBurstProAir.cpp)：Roccat；上游 Direct 标记 :white_check_mark:
- [Roccat Kone Aimo](https://codeberg.org/OpenRGB/OpenRGB/src/tag/release_1.1/Controllers/RoccatController/RoccatKoneAimoController/RGBController_RoccatKoneAimo.cpp)：Roccat；上游 Direct 标记 :white_check_mark:
- [Roccat Kone Pro Air Mouse](https://codeberg.org/OpenRGB/OpenRGB/src/tag/release_1.1/Controllers/RoccatController/RoccatKoneProAirController/RGBController_RoccatKoneProAir.cpp)：Roccat；上游 Direct 标记 :white_check_mark:
- [Roccat Kone Pro Mouse](https://codeberg.org/OpenRGB/OpenRGB/src/tag/release_1.1/Controllers/RoccatController/RoccatKoneProController/RGBController_RoccatKonePro.cpp)：Roccat；上游 Direct 标记 :white_check_mark:
- [Roccat Kone XP Mouse](https://codeberg.org/OpenRGB/OpenRGB/src/tag/release_1.1/Controllers/RoccatController/RoccatKoneXPController/RGBController_RoccatKoneXP.cpp)：Roccat；上游 Direct 标记 :white_check_mark:
- [Roccat Kova](https://codeberg.org/OpenRGB/OpenRGB/src/tag/release_1.1/Controllers/RoccatController/RoccatKovaController/RGBController_RoccatKova.cpp)：Roccat；上游 Direct 标记 :x:
- [Sinowealth 1007 Mouse](https://codeberg.org/OpenRGB/OpenRGB/src/tag/release_1.1/Controllers/SinowealthController/Sinowealth1007Controller/RGBController_Sinowealth1007.cpp)：ZET；上游 Direct 标记 :x:
- [Sinowealth Glorious Model O Wireless](https://codeberg.org/OpenRGB/OpenRGB/src/tag/release_1.1/Controllers/SinowealthController/SinowealthGMOWController/RGBController_SinowealthGMOW.cpp)：品牌由设备返回；上游 Direct 标记 :x:
- [Sinowealth Mice](https://codeberg.org/OpenRGB/OpenRGB/src/tag/release_1.1/Controllers/SinowealthController/SinowealthController/RGBController_Sinowealth.cpp)：品牌由设备返回；上游 Direct 标记 :x:
- [Steel Series Rival](https://codeberg.org/OpenRGB/OpenRGB/src/tag/release_1.1/Controllers/SteelSeriesController/SteelSeriesRivalController/RGBController_SteelSeriesRival.cpp)：SteelSeries；上游 Direct 标记 :white_check_mark:
- [Steel Series Rival 3](https://codeberg.org/OpenRGB/OpenRGB/src/tag/release_1.1/Controllers/SteelSeriesController/SteelSeriesRival3Controller/RGBController_SteelSeriesRival3.cpp)：SteelSeries；上游 Direct 标记 :white_check_mark:
- [Steel Series Sensei](https://codeberg.org/OpenRGB/OpenRGB/src/tag/release_1.1/Controllers/SteelSeriesController/SteelSeriesSenseiController/RGBController_SteelSeriesSensei.cpp)：SteelSeries；上游 Direct 标记 :white_check_mark:
- [SteelSeriesRival5](https://codeberg.org/OpenRGB/OpenRGB/src/tag/release_1.1/Controllers/SteelSeriesController/SteelSeriesRival5Controller/RGBController_SteelSeriesRival5.cpp)：SteelSeries；上游 Direct 标记 以连接后的模式为准
- [Tecknet Mouse](https://codeberg.org/OpenRGB/OpenRGB/src/tag/release_1.1/Controllers/TecknetController/RGBController_Tecknet.cpp)：Tecknet；上游 Direct 标记 :white_check_mark:
- [Trust GXT 114](https://codeberg.org/OpenRGB/OpenRGB/src/tag/release_1.1/Controllers/TrustController/TrustGXT114Controller/RGBController_TrustGXT114.cpp)：Trust；上游 Direct 标记 :x:
- [Trust GXT 180](https://codeberg.org/OpenRGB/OpenRGB/src/tag/release_1.1/Controllers/TrustController/TrustGXT180Controller/RGBController_TrustGXT180.cpp)：Trust；上游 Direct 标记 :x:

## 仍待完成

未公开的品牌协议、DPI/按键/宏导入、官方私有灯效、固件升级和未列型号仍需逐个研究与硬件测试。没有适配器的设备只列调查线索，不能控制。内置 OpenRGB 的驱动和系统权限限制依旧适用。
