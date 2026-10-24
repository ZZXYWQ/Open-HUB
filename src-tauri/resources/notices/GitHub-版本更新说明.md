# GitHub 更新

Open HUB 0.3.2 起使用本分支的公开更新元数据：
https://raw.githubusercontent.com/ZZXYWQ/Open-HUB/main/updates/latest.json

程序启动约 30 秒后、随后每 6 小时查询一次；外观与设置里也可手动检查或关闭自动检查。发现新版本会显示版本、实验版标识和更新说明。只有用户点击下载才获取安装包；完成下载后还需明确点击安装并重启，可以稍后再装。托盘也是先下载、再由第二次明确选择安装。没有自动下载或自动安装选项；旧配置里的 auto_install_updates=true 在读入和保存时归一为 false。

安装包只接受本仓库 GitHub Release 的版本对应文件。Tauri 更新器使用 Open HUB 独立公钥验证文件签名，并要求签名内的版本与元数据一致。下载完成时验证，安装前再次验证。失败不会启动安装程序，缓存损坏可重新下载。更新缓存位于应用缓存目录，不覆盖个人配置。GitHub 无法连接会报告错误，不显示为“已是最新版本”。

版本 0.3.1 的旧客户端没有启用此更新源，因此第一次需要手动安装 0.3.2。自动查询的是这份经过发布流程更新的索引，可包含明确标注的实验版；不会把 GitHub 不相关仓库或普通附件当作更新。

更新签名与 Windows Authenticode 证书不同，本实验 EXE 尚无 Windows 代码签名。私钥只保存在本机项目外，不在仓库、源码包或安装包中。保留该私钥才能为以后版本签名，不应随意轮换公钥。

## 本地发布

1. 统一版本号，完成原生、前端、安装器验证并准备说明。
2. 通过本机环境提供 TAURI_SIGNING_PRIVATE_KEY_PATH；运行 scripts/prepare-experimental-release.mjs，明确传入 --installer、--version、--tag、--repo、--notes 和 --out。脚本使用 Tauri 官方 signer 的 --app-version，生成版本绑定签名、latest.json 和哈希。不要提交或打印私钥。
3. 将同一 EXE、.sig、对应源码、校验文件上传到本仓库实验 Release 并发布。
4. 仅在附件可匿名下载后，将生成的 latest.json 原样提交到 updates/latest.json；校验公开 URL 和签名。
5. 可用构建后的 open-hub.exe --verify-release-update <report.json> 只读验收公开更新：下载并验证真实签名，同时确认改动文件和伪造版本均被拒绝。此模式不会安装、访问鼠标、加载用户配置或启动普通窗口。

本版没有新增自动发版工作流，不需要 workflow OAuth 权限。
