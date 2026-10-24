# 构建与测试

代码内部版本为 0.3.1，GitHub 预发布标签为 v0.3.1-experimental.1。

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

浏览器脚本 scripts/test-*-ui.cjs 另需 Playwright 与 Chromium 开发依赖，不进入应用运行时。脚本模拟 Tauri，一些回归需要先由原生测试生成 DTO fixture。scripts/test-installer-migration.py 另需 NSIS，执行隔离样本，不等于整包安装测试。保留脚本中的可选外部 fixture / 研究输入路径，不将它们伪装成随包提供的输入。

实际证据见 RELEASE-VALIDATION.md、docs/releases/0.3.1-validation.json。模拟测试不能代替实机。

本次手动发布已验证安装器，没有配置本分支的自动工作流、签名密钥或自动发版。上游工作流作为原历史保留，不继承其签名声明。
