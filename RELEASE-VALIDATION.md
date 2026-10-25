# Open HUB 0.3.5 按键修复验证 — 2026-10-25

只保留每只鼠标的独立配置，没有加入全局鼠标映射。本次补充 Pulsar X2 V2 Mini 的基础按键，以及 Viper V3 Pro 的滚轮方向/底部 DPI 原动作恢复和禁用；具体范围见 [按键说明](BUTTON-COMPATIBILITY.md)。这不是全部鼠标或全部按键兼容。

299 项主程序默认测试、5 项公开输入/界面保存方案交叉验证、79 项雷蛇库测试通过。新增用例核对独立公开按键向量、回读失败恢复、未知原动作锁定、错误动作拒绝、同型号其他实例保持原配置。界面使用生产驱动生成的当前 DTO，HyperX/Pulsar 六份、Viper 六份、上一批五品牌十份保存方案交回原生驱动执行。完整 20 组浏览器回归通过，IPC 模拟不代表实机。

EXE/安装包载荷、当前前端嵌入、独立版本绑定签名与公开下载记录见 [本版验证](docs/releases/0.3.5-validation.json) 和发布后的 [公开验收](docs/releases/0.3.5-public-verification.json)。没有新增型号实机测试。本机升级按用户授权执行，配置备份保持私有；其他用户仍自行选择下载及安装。

## 历史验证记录

# Open HUB 0.3.4 验证 — 2026-10-25

新增五个品牌各一款有线型号：ASUS ROG Strix Impact、Roccat Kone Pure、G.Skill MX-780、Etekcity Scroll Alpha、Mars Gaming MM4。精确 USB ID 和功能限制见 FIVE-BRAND-ADAPTERS.md；合计 23 个品牌的限定型号。未新增实机验收，不作整品牌兼容声明。

292 项默认原生回归及 4 项显式交叉验证通过。新增五品牌生产驱动在内存 HID 上运行；真实驱动生成的 DTO 进入浏览器读入/编辑/保存流程，十份界面保存方案再经过实际 Rust 配置验证、反序列化及原生驱动执行。20 组浏览器套件通过，包括旧品牌导入和灯光、更新、主题、恢复、型号图等。图片目录覆盖 61 个原生身份，新增结构示意在界面明确标注，不冒充照片。

保护测试覆盖精确型号/厂商控制集合、短读/读取失败、非法 DPI/回报率、硬件组变化、左键保护、未知原厂宏保留、被其他控制器改变的宏（包括未刷新直接应用）、原生宏载荷回读、三种不同宏限制、ASUS 非标准键码、整报告内不相关 RGB/名称/按键字节保留以及写入中断恢复。缺少硬件，不能验证 Windows HID 控制集合、真实固件响应与 EEPROM 持久性。

生产构建、安装包载荷、迁移夹具、版本绑定签名和公开下载验证记录见 docs/releases/0.3.4-validation.json 与发布后的 public-verification.json。本机安装遵循用户明确要求，升级前备份配置与偏好、核对载荷并启动确认；个人配置/备份/路径不写入公开发布材料。应用内更新仍由每位用户选择下载与安装。

## 历史验证记录

# Open HUB 0.3.3 验证 — 2026-10-25

新增 HyperX Haste 两型号和 Pulsar X2 V2 Mini 两种连接。支持范围以 HYPERX-ADAPTER.md / PULSAR-ADAPTER.md 为准，不是整品牌兼容。实机仍只有既有 MCHOSE A7 V3 Ultra+ / MagDock；本轮未访问鼠标硬件。

273 项默认原生回归，加显式提供公开 GG/iCUE 输入及六份 UI 创建方案的三项测试；19 组浏览器 UI 回归。新协议使用生产驱动配合内存 HID，UI 使用原生驱动导出的四组 DTO。六份界面导入/编辑保存的文件再经配置校验、序列化和生产驱动模拟执行。公开 HyperX USBPcap 载荷独立核对回报率、按键输出及宏格式，不把模拟测试当作实机验证。

异常覆盖硬件身份错误、短输出、截断/不相关输入、稀疏 DPI 档位、失败后的原值恢复、未知原厂宏/跨按键宏槽引用、越界参数、Pulsar 配置组变化、未支持动作及原有品牌隔离。HyperX 只检查宏绑定读回，不能读回宏内容；灯色只保留会话值。Pulsar 不开放按键和宏写入。

生产构建、安装包逐文件哈希、独立安装器迁移夹具和签名验证见 docs/releases/0.3.3-validation.json。没有对新增鼠标作实机、全新 Windows 或缺失 WebView2 补装验收；本轮不自动升级用户正在运行的 0.3.2。

## 历史验证记录

# Open HUB 0.3.2 — VXE/VGN、型号外观、用户选择更新

新增六款精确 VXE/VGN USB 身份，范围与 MID 白名单见 COMPX-ADAPTER.md。生产驱动模拟覆盖 DPI/按键、三种板载宏、原厂未知宏保留、五键 MAD R、型号改变和共享 PID 未实现 MID 拒绝；没有新增实机验证。原生回归合计 248 项通过：默认运行 246 项，另显式提供公开 iCUE 样本和官方 GG SQL 后执行的两项也通过。

所有已命名鼠标的设备中心/原生设置/方案与原生灯光卡片使用型号图片，25 张 WebP + 18 张可编辑 SVG 合计 668,716 字节。52 个原生目录身份的外观显示/资源解码回归覆盖 25 个罗技名称、11 条 SinoWealth 型号/固件、9 条 COMPX 连接身份和 7 个其他原生型号；这些不是新增 52 款硬件兼容声明。迈从已有图片压缩与原页面保留。

新增浏览器回归验证 VXE/VGN 独立品牌筛选、板载导入的宏/XY DPI/精确目标、错误设备响应拒绝、六款原生 DTO 的 UI 与配色、五键/回报率能力边界，以及更新检查/稍后无下载、签名失败、单独下载与安装选择、旧自动安装选项归零和离线错误。全部界面套件与最终包验收以 docs/releases/0.3.2-validation.json 为准。浏览器只模拟 Tauri IPC，不冒充硬件执行。

更新源仅本仓库，版本绑定签名且安装前二次校验。发布后的生产 EXE 只读诊断已通过：公开附件签名有效，篡改文件与版本不符均被拒绝；没有启动安装器。匿名完整下载哈希一致，见 docs/releases/0.3.2-public-verification.json。当前机器不会因发布动作自动升级。0.3.1 升级安装保留数据的历史证据继续有效，但不等同本次 0.3.2 新安装实测。

本次不声称新增型号实机通过、全品牌兼容、所有官方配置可解析或任意 RGB 灯效可用。Windows EXE 仍没有 Authenticode 代码签名。上游共享驱动与导入解析器未修改，原有历史验证如下。

# Open HUB 0.3.1 ATK validation — 2026-10-24

ATK adds A9 Plus NK USB 373B:1115 and older NearLink 373B:10C9 (CID/MID 2,83), and X1 Pro Max USB 373B:1017 (2,39). These are two mouse models / three exact transport identities, not whole-brand compatibility. ATK has no physical-device validation. Scope and protocol provenance: ATK-ADAPTER.md.

239 native regressions passed, including 19 ATK tests. ATK tests exercise the production driver over synthetic HID, native macro upload/readback/re-import, all three playback modes, no-free-slot preservation, interrupted-upload restoration, corrupt/truncated/status/identity replies, left-click protection, 8-stage DPI, exact rate ceilings, static indicator colors, and bridge serialization/execution. All 15 browser UI suites passed; ATK UI checks read-only import, stale-preview clearing, exact-device mismatch rejection, macro contents/mode, no auto-application, and native macro editing/simulation. Installation evidence is recorded separately in the delivered verification JSON.

The optional Device native-macro methods have default implementations; prior drivers do not receive ATK macro definitions. No new runtime dependency or official control center is bundled. App identifier and user configuration locations are unchanged.

Appearance adds three native schema/export tests and a browser suite covering color presets, validated 8 KB skin imports/exports, persistence, background alpha without fading text, minimum-size reachability and device-write isolation. The new abstract Z-motion icon is used by the header and all app/installer/tray assets. See APPEARANCE.md and docs/BRANDING.md.

## Previous 0.3.0 release evidence

# Open HUB 0.3.0 validation — 2026-10-24

## Scope and limits

The completed integration scope is three exact models: Razer Naga V2 HyperSpeed
1532:00B4, Corsair M65 PRO RGB 1B1C:1B2E, and SteelSeries Rival 310 1038:1720.
It includes supported-source conversion, on-disk full-profile persistence,
DPI/rate, supported button actions, host macro execution and corresponding UI.
Unsupported source actions/layouts are blocked as a whole candidate. This is not
whole-brand support. MOUSE-BRAND-SUPPORT.md is the model/capability matrix.

The new models use real protocol drivers with synthetic HID/input sinks. No
physical verification is claimed for them. Existing MCHOSE A7 V3 Ultra+/MagDock
are the available mouse/dock hardware. Browser suites mock Tauri and consume
native DTO fixtures where applicable; they are separate evidence from native
source-to-output tests, not a claim that browser mocks execute native code.

## Requirement audit

| Requirement | Authoritative implementation and validation |
|---|---|
| Naga source import/save and executable controls | `synapse_profiles.rs`, Naga production driver, `import_execution_tests::naga_synapse_file_disk_reload_native_trigger_and_macro_output`; actual Synapse 4 JSON/base64 file entry -> disk store -> HID driver -> physical-source dispatch -> complete A/B hold output |
| M65 source import/save and executable controls | `icue_profiles.rs`, Corsair NXP driver, `m65_icue_file_disk_reload_vendor_button_and_macro_output`; iCUE XML entry -> disk -> vendor-button dispatch -> full macro output. An absent source polling rate stays absent; the session view may reflect the target's known rate |
| Rival source import/save and executable controls | `gg_profiles.rs`, SteelSeries driver, `rival_gg_database_disk_reload_native_trigger_and_mixed_macro_output`; SQLite source -> disk -> native trigger -> mixed keyboard/mouse output |
| Profile persistence and editing | Production ProfileStore and merge validation are used by all three cross-layer tests. `session_plan.rs` verifies full desired plan across read/edit/reapply, temporary DPI handling, removed sources, serial/model isolation and scoped stop |
| UI and configuration imports | All 13 rendered browser suites passed. Brand chooser has 20 checks, 13 individual supported-brand entries, one toolbar entry, bounded narrow-window layout and source-specific dialogs. Import suites cover edit/save, macro modes, invalid sources and foreign models |
| Logitech/MCHOSE isolation | Backend filters/exact VID/PID validation; tests reject plans before unrelated cleanup or writes. Cross-layer stores include both legacy brands and assert byte-equivalent serialized profiles. Foreign source/model/serial and scoped-stop regressions pass |
| Fault handling | Native suites cover malformed/unbalanced macros, held keys, cancellation, disconnect, concurrent ownership, write-ahead failure, stale/foreign recovery, changed polling re-enumeration, unknown send results and reviewed retries. `manual_recovery.rs`, `rate_reconnect.rs`, `macro_recovery.rs`, `macro_runtime.rs` and driver tests implement these paths |
| Installer and migration | Production NSIS is extracted and all 35 configured executable/loader/resource payload files compared by SHA256. Actual 0.2.3 upgrade, same-version overwrite, uninstall retaining data, absent-directory/registration reinstall and restart pass. Original user files remain byte-identical. Shared production NSIS macro also passes nested/Unicode/repeat/copy-sharing-failure/destination-failure/missing-source fixtures |
| Compact distribution and licensing | Normal installer is about 39 MiB with system/downloaded WebView2; offline runtime is opt-in. Shared OpenRGB and source/license resources remain. Research installers, vendor applications, build outputs and tools are excluded. Corresponding source archive is verified entry by entry |

## Final regression evidence

- `../final-native-import-execution-regression.log`: **217 passed, 0 failed, 0 ignored**.
- `../brand-chooser-final-vendor-tests.log`: **289 unit + 1 documentation test passed**.
- `../final-synapse-parser-regression.log`: **148 unit + 3 integration tests passed**, including explicit public-source smoke input.
- `../browser-ui-regression-report.json`: all **13 suites passed**; source/UI code is unchanged after the last UI run.
- `../source-import-execution-tests.log`: the three new source-file/disk/native-trigger/output tests passed.
- `../installer-migration-qa-23e42286/report.json`: compiled production NSIS macro runtime fixtures passed.
- `../final-installer-lifecycle-verification.json`: actual installer lifecycle and data preservation.

External GG/iCUE research probes are opt-in tests. Their explicit input variables
are `OPEN_HUB_GG_RIVAL310_SQL` and `OPEN_HUB_ICUE_SAMPLE`; the release run set both
to existing inert research sources. Parser smoke uses
`SIDEARM_SYNAPSE_SMOKE_FILE`, set to the bundled public fixture. No vendor SQL or
proprietary executable is run. Source fixtures in `tests/fixtures` and generated
M65/GG test data are synthetic, not physical-device captures.

## Installation verification boundary

The clean installation test starts with no Open HUB program directory or uninstall
registration, while retaining the user's configuration as required. It reuses the
current Windows user and existing WebView2. A pristine Windows VM/new-user first
run and the missing-WebView2 network bootstrap were not exercised. This is an
explicit limitation, separate from the verified repeat-install requirement.

Final user artifacts and SHA256 values are recorded next to the installer in
`../../outputs/Open-HUB-0.3.0-brand-imports-verification.json`. The source archive
includes the final protocol code, tests, synthetic fixture, resources and licenses.
