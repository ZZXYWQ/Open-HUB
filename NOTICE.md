# 来源与第三方组件

本项目是基于 Abidur Rahman 的 [Open GHub](https://github.com/abidr/open-ghub) 修改的独立实验分支，保留上游 Git 历史与 [GPL-3.0-or-later](LICENSE)。基线：2853a6251a31a376a0f7287c1c846bd9bff97198。修改日期截至 2026-10-26。

新增多品牌适配与隔离、配置导入、板载/软件宏、灯光、外观和新图标；不代表上游或硬件厂商认证。

|组件|来源 / 许可|对应文件|
|---|---|---|
|Open GHub|Abidur Rahman；GPL-3.0-or-later|LICENSE、原提交历史、docs/upstream/Open-GHub-README.md|
|Gearwise|https://github.com/Dhiva-Labs/gearwise ；MIT|src-tauri/vendor/gearwise、Gearwise-MIT.txt|
|Sidearm Synapse parser|MIT，原始署名保留|src-tauri/vendor/sidearm-synapse-parser、Sidearm-MIT.txt|
|libratbag SinoWealth 参考|MIT；Marian Beermann © 2020|libratbag-MIT-license.txt、参考源文件|
|HyperX Haste 协议|Evan Razzaque；MIT 驱动参考，另行核对其 GPL-3.0 公开抓包/协议事实|HYPERX-ADAPTER.md、HyperX-Protocol-MIT.txt、HyperX-Source-GPL-3.0.txt|
|Pulsar X2 V2 Mini 协议|Andrew Rabert；MIT|PULSAR-ADAPTER.md、Pulsar-Protocol-MIT.txt|
|OpenRGB 1.1|GPL-2.0-or-later，未修改可携组件|src-tauri/resources/rgb-engine、OpenRGB-LICENSE.txt、OpenRGB-1.1-source.tar.gz|
|Qt 等 OpenRGB 动态组件|保留原组件许可；Qt 源码 https://download.qt.io/archive/qt/ |随包第三方组件说明和动态库|
|MCHOSE 设备图片|官方 CDN；权利属于原权利人，用于设备识别|public 与随包第三方组件说明|
|Open HUB 抽象 Z 图标|本分支独立生成，不是 Open GHub 原图标|docs/BRANDING.md、docs/branding|

ATK 协议按官方公开网页驱动和协议事实独立实现，来源与边界见 ATK-ADAPTER.md。没有分发厂商控制中心或完整官方网页应用作为适配器。

Windows EXE 尚未取得 Authenticode 代码签名。0.3.2 起从本仓库 GitHub 更新源检查版本，下载与安装都由用户选择；更新包使用独立密钥与版本绑定签名，上游 SignPath 声明不适用。

型号产品图的来源、权利说明与哈希见 docs/DEVICE-ARTWORK.md 和 docs/DEVICE-ARTWORK-SOURCES.json。Piper 的 20 张未修改 SVG 按其原 GPL-2.0 单独保留许可与可编辑源文件，见 docs/PIPER-ARTWORK.md 和 Piper-SVG-COPYING.txt；没有重许可为应用代码许可，也没有包含 Piper/GTK 运行库。

HyperX Haste SVG 的 GPL-3.0 许可与可编辑源文件单独保留，协议作者及固定提交见 HYPERX-ADAPTER.md。公开 USB 抓包只提取报文与来源哈希，未包含用户私有输入记录。

Haste SVG 仅去除独立标注引线层并收紧画布，主体路径未改；原始 SVG 另存 docs/upstream/HyperX-Pulsefire-Haste-original.svg，保留原作者与 GPL-3.0 许可。

Five wired model adapters: libratbag MIT reference, fixed source and scope in [FIVE-BRAND-ADAPTERS.md](FIVE-BRAND-ADAPTERS.md), bundled copyright notices in Wired-Protocol-MIT.txt. Two additional unmodified Piper SVGs retain GPL-2.0; three original lightweight structural illustrations are labelled as illustrations.

Pulsar Nordic keyboard shortcut records: independently implemented using the MIT reference by Packerlschupfer, fixed commit a18ed8fb963a296d4c5493f51017df32f30e975c of https://github.com/packerlschupfer/pulsar-mouse-linux . Copyright and MIT permission text are bundled in Pulsar-Nordic-MIT.txt. No Python/GTK runtime or official vendor application is bundled.

Basilisk V3 Pro button protocol: independently implemented from kev-landry/basilisk-configurator commit 2bb8b3c81a201465e2b888bafa46b6cbb05ad9ae (MIT, Kevin Landry). Scope and evidence in BASILISK-ADAPTER.md; original license bundled as Basilisk-Configurator-MIT.txt. No vendor runtime or application bundled.

Attack Shark X11 codec, table and test capture reference: HarukaYamamoto0 (MIT), akawazak/attack-shark-x11-community-driver commit 49ecf8e6d3bd83bf71e231209d28c5e0e8b71d12. Original permission text: AttackShark-X11-MIT.txt. Scope, contradictions and readback limits: [docs/X11-ADAPTER.md](docs/X11-ADAPTER.md). No vendor runtime, WinUSB replacement, or factory application is bundled.

Logitech native lighting protocol cross-reference: infinitysky/LogitechGDeviceInfo (MIT, copyright 2026 infinitySKYLi), fixed commit e19565d9f14d23c7e7c8eb6b674c3de907243515. Original permission text is bundled as Logitray-MIT.txt. Device discovery, per-device identity, recovery and UI are independently integrated; no third-party executable or runtime is bundled.
