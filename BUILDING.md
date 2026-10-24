# 构建与测试

代码内部版本为 0.3.2，GitHub 预发布标签为 v0.3.2-experimental.1。

## Windows 构建

需要 Node.js 22、npm、Rust 和 Windows 工具链；MSVC 需要 Visual Studio C++ Build Tools 与 Windows SDK。安装应用需要 WebView2。仓库包含 OpenRGB 可携组件、资源、许可及对应源码。

```powershell
npm ci
npm run build
npm run app:build -- --bundles nsis
```

常规包复用系统 WebView2；完整离线版为 `npm run app:build:offline`。NSIS 模板和升级钩子在 installer/，配置为 src-tauri/tauri.conf.json。

已发布 EXE 使用 GNU Windows、LLD 和以下 release 覆盖参数构建；其他工具链不承诺产物逐字节相同：

```powershell
# 先配置 GNU GCC / LLD、cargo、rustup
$env:CARGO_PROFILE_RELEASE_LTO = 'false'
$env:CARGO_PROFILE_RELEASE_CODEGEN_UNITS = '16'
Set-Location src-tauri
cargo rustc --release --features tauri/custom-protocol -- -C opt-level=1 -C link-arg=-fuse-ld=lld
Set-Location ..
npx tauri bundle --bundles nsis --ci --no-sign --no-binary-patching
```

GNU 构建还需要 Tauri 安装器提供的 WebView2Loader.dll。不要把本机绝对工具链路径写入配置。`npm run package:footprint` 可检查资源体积。

## 测试

```powershell
npm ci
npm run build
cargo test --locked --manifest-path src-tauri/Cargo.toml
cargo test --locked --manifest-path src-tauri/vendor/gearwise/Cargo.toml --workspace
cargo test --locked --manifest-path src-tauri/vendor/sidearm-synapse-parser/Cargo.toml
```

浏览器脚本 scripts/test-*-ui.cjs 另需 Playwright 与可用浏览器开发依赖（Windows 测试运行器可复用已安装 Edge），不进入应用运行时。脚本模拟 Tauri，一些回归需要先由原生测试生成 DTO fixture。scripts/test-installer-migration.py 另需 NSIS，执行隔离样本，不等于整包安装测试。保留脚本中的可选外部 fixture / 研究输入路径，不将它们伪装成随包提供的输入。

实际证据见 RELEASE-VALIDATION.md、docs/releases/0.3.2-validation.json。模拟测试不能代替实机。

本次仍为本地手动构建和发布，未修改上游工作流。更新包使用本分支独立私钥的版本绑定签名，签名发布流程见 docs/UPDATES.md；Windows EXE 没有 Authenticode 证书。

## 原生 DTO 和 UI 回归

在运行根原生测试前，可将 OPEN_HUB_COMPX_FIXTURE_DIR 设为 ../outputs/compx-native-fixtures 的绝对路径，OPEN_HUB_SESSION_FIXTURE 设为 ../native-session-views.json，OPEN_HUB_VIPER_FIXTURE 设为 ../native-viper-views.json 的绝对路径，生成明确的合成 HID 输出。运行 node scripts/run-ui-regression.cjs 验证已构建的 dist，需要 Playwright 开发环境；这些包不进入运行时。可选 OPEN_HUB_ICUE_SAMPLE / OPEN_HUB_GG_RIVAL310_SQL 测试需要另行获取对应公开样本/官方安装包中的 SQL，不在源码包附带厂商安装程序。
