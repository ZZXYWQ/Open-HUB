# 来源与第三方组件

本项目是基于 Abidur Rahman 的 [Open GHub](https://github.com/abidr/open-ghub) 修改的独立实验分支，保留上游 Git 历史与 [GPL-3.0-or-later](LICENSE)。基线：2853a6251a31a376a0f7287c1c846bd9bff97198。修改日期截至 2026-10-25。

新增多品牌适配与隔离、配置导入、板载/软件宏、灯光、外观和新图标；不代表上游或硬件厂商认证。

|组件|来源 / 许可|对应文件|
|---|---|---|
|Open GHub|Abidur Rahman；GPL-3.0-or-later|LICENSE、原提交历史、docs/upstream/Open-GHub-README.md|
|Gearwise|https://github.com/Dhiva-Labs/gearwise ；MIT|src-tauri/vendor/gearwise、Gearwise-MIT.txt|
|Sidearm Synapse parser|MIT，原始署名保留|src-tauri/vendor/sidearm-synapse-parser、Sidearm-MIT.txt|
|libratbag SinoWealth 参考|MIT；Marian Beermann © 2020|libratbag-MIT-license.txt、参考源文件|
|OpenRGB 1.1|GPL-2.0-or-later，未修改可携组件|src-tauri/resources/rgb-engine、OpenRGB-LICENSE.txt、OpenRGB-1.1-source.tar.gz|
|Qt 等 OpenRGB 动态组件|保留原组件许可；Qt 源码 https://download.qt.io/archive/qt/ |随包第三方组件说明和动态库|
|MCHOSE 设备图片|官方 CDN；权利属于原权利人，用于设备识别|public 与随包第三方组件说明|
|Open HUB 抽象 Z 图标|本分支独立生成，不是 Open GHub 原图标|docs/BRANDING.md、docs/branding|

ATK 协议按官方公开网页驱动和协议事实独立实现，来源与边界见 ATK-ADAPTER.md。没有分发厂商控制中心或完整官方网页应用作为适配器。

Windows EXE 尚未取得 Authenticode 代码签名。0.3.2 起从本仓库 GitHub 更新源检查版本，下载与安装都由用户选择；更新包使用独立密钥与版本绑定签名，上游 SignPath 声明不适用。

型号产品图的来源、权利说明与哈希见 docs/DEVICE-ARTWORK.md 和 docs/DEVICE-ARTWORK-SOURCES.json。Piper 的 18 张未修改 SVG 按其原 GPL-2.0 单独保留许可与可编辑源文件，见 docs/PIPER-ARTWORK.md 和 Piper-SVG-COPYING.txt；没有重许可为应用代码许可，也没有包含 Piper/GTK 运行库。
