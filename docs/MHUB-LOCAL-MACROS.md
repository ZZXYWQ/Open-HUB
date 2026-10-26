# M HUB 本地宏正文只读导入

开发分支增量，未发布或安装。现有 A7 V3 Ultra+ 完整方案导入可选择“使用本机 M HUB 宏库”，从 M HUB 自己的本地配置读取方案引用的宏正文；手动选择 JSON 宏库文件继续保留。没有增加品牌、鼠标型号、灯效或完整验收计数。

## 读取范围与格式证据

[原厂与数据库格式事实](development/mhub-local-storage-official-facts.json)记录固定渲染器 SHA-256、静态代码位置、格式来源及本机只读观察。原厂把 `macroListM7` 放在 Local Storage，值是 JSON 宏记录数组；IndexedDB 的一般设备配置类不是这份宏库。没有运行原厂 JS、字节码或原厂程序。

生产代码只定位 `%APPDATA%/MCHOSEHUB/Local Storage/leveldb`。向导入层返回 `VERSION` 和三个已知候选来源中严格匹配的 `macroListM7` 记录；不会到 Chrome/Edge 用户目录里找同名键。其他数据库内容只经过容器结构与校验处理，不作为应用字段解释、输出或保存。候选来源标记只表示软件存储位置，不表示无线鼠标身份。

Chromium 的[Local Storage 格式](https://github.com/chromium/chromium/blob/131.0.6778.0/components/services/storage/dom_storage/local_storage_impl.cc)规定 schema 1 和来源/键前缀；[Blink 字符串格式](https://github.com/chromium/chromium/blob/131.0.6778.0/third_party/blink/renderer/modules/storage/cached_storage_area.cc)区分 UTF-16 与 Latin-1。中文和补充字符经过严格解码，无效代理项拒绝；同一宏键同时有多种编码记录时不猜测当前内容。

## 当前记录判定

实现是小型只读解析器，不启动可能进行恢复和写入的数据库引擎，不创建 LOCK、日志或临时数据库。按照 [LevelDB 日志](https://github.com/google/leveldb/blob/main/doc/log_format.md)、[SST](https://github.com/google/leveldb/blob/main/doc/table_format.md)、[manifest](https://github.com/google/leveldb/blob/main/db/version_edit.cc) 和 [WriteBatch](https://github.com/google/leveldb/blob/main/db/write_batch.cc) 格式读取 CURRENT 指向的 manifest、当前活跃表与恢复日志，使用内部 sequence 选择较新的 put/delete。孤立旧表和过期日志不会作为正文来源。

日志与表块检查 masked CRC32C；分片完整性、前缀/重启表、偏移、长度和整数边界必须有效。按 [Snappy 格式](https://github.com/google/snappy/blob/main/format_description.txt) 解码压缩块；不支持的压缩类型、比较器、schema 或损坏内容直接拒绝，不通过扫描旧字节找“看起来像宏”的数据。

单文件最多 32 MiB、参与文件总量最多 64 MiB、每次读取的表块解压总量最多 128 MiB、解码宏库最多 8 MiB；对表和日志数量也有限制。参与文件、manifest/CURRENT 和日志集合经过重复读取与一致性核对；变化时拒绝本次预览。它仍是已落盘状态的读取，不保证与正在运行的 M HUB 内存或未保存编辑一致。模板只保存名称，因此选择当前库也不能证明正文与历史导出时相同；导入前可检查完整事件。

自动选择只接受唯一来源记录。多个来源、已删除宏库、同名宏正文歧义等情况不会自动合并、恢复或切到别处；用户可以明确选择方案对应的来源，或改用宏库文件。读取结果包含来源和正文 SHA-256，界面显示来源；再经过原有严格 JSON、完整 Plan、宏事件与摘要校验进入配置中心。

## 验证与边界

[验证记录](development/2026-10-27-mhub-local-validation.json)区分以下证据：

- 独立 CRC/Snappy 向量、分片/尾部/前缀/长度错误、更新/删除/冲突、来源/编码歧义，以及原文件不变性单元测试。
- 隔离 Chromium/Edge 配置仅运行本项目空白测试页，生成真实初始 WAL、压缩索引 SST 加新 WAL、删除宏库后的 WAL。读取结果逐字节匹配测试页写入的 JSON，排除其他来源同名键与无关哨兵；未执行原厂代码。
- 实际构建的 React 导入界面调用真正 Rust 读取/转换函数，验证自动来源、显式来源、删除和无记录的拒绝、旧预览清空及手动文件切换。保存出的两份同名不同正文方案经过生产差异事务回放，核对已选正文、原有宏保护和另一只鼠标零通信。
- 本机已安装 M HUB 的只读探测可解析现有压缩表格式，全部文件前后哈希一致；没有目标宏正文键，因此没有据此声称取得实际用户宏或验证了实际播放。

仍缺用户实际宏库样本、实物播放/固件行为、RF 单元身份、完整应用回执、未转换动作和逐型号功能/灯效验收。没有将只读库解析或浏览器模拟算作新鼠标完整适配。
