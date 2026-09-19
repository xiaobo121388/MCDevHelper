# Windows 测试打包器

`MCDH-TestPacker.exe` 是独立 Windows x64 程序，不需要 Python、Node.js 或 PowerShell。它只读取输入目录，不修改项目文件，不联网，不启动其他程序。

## 最简单的配置

在 MCDH 的“设置 > 自定义导出”新增方案：

| 字段 | 填写内容 |
|---|---|
| 按钮名称 | 测试打包 |
| 程序 | 本说明旁边的 `MCDH-TestPacker.exe`，选择完整绝对路径 |
| 参数 | 留空 |
| 输入方式 | 完整临时副本 |
| 工作目录 | 留空 |
| 日志编码 | UTF-8 |
| 超时 | 1800 秒 |
| 适用类型 | 按需勾选，默认全部 |
| 允许 MCP 执行 | 手工测试不需要开启 |

保存后进入组件配置面板，选择导出目录并点击“测试打包”。预期先看到进度日志、中文 UTF-8 日志和一条 stderr 诊断日志，约 1.5 秒后开始压缩，最终得到 `mcdh-test.zip`。大项目的实际压缩时间可能更长。stderr 的诊断日志是刻意输出的，不代表任务失败。

ZIP 保留普通文件、点号文件、Python 开发文件和空目录，跳过符号链接及目录联接；不应用内置游戏包清洁规则。它用于测试导出接入，不保证产物可直接用于游戏发布。

## 测试模式

要切换模式，在参数列表中添加两条独立参数：第一条 `--mode`，第二条填写下表中的值。不要把两条合成一条，也不需要额外加引号。

| 第二条参数 | 预期结果 |
|---|---|
| `success` | 默认模式，生成唯一 ZIP，导出成功 |
| `slow` | 压缩前等待 60 秒，每秒输出日志，适合测试“取消任务” |
| `fail` | 输出错误日志，以退出码 23 结束，不生成产物 |
| `empty` | 退出码 0，但没有产物；MCDH 应拒绝导出 |
| `multiple` | 生成 ZIP 和一个额外报告；MCDH 应拒绝多个产物 |
| `zero` | 生成一个零字节 ZIP；MCDH 应拒绝空文件 |
| `logs` | 输出超过 2 MiB 日志后生成 ZIP；应成功且提示日志截断 |

重名测试：成功导出后，对同一目标目录再次导出，应提示文件重名。选择“添加后缀”得到 `mcdh-test (2).zip`；选择覆盖则替换原文件。重名处理不应再次出现完整打包过程。

超时测试：使用 `slow` 模式，将方案超时设为 `3` 秒，应显示超时且不发布最终文件。测试完成后恢复正常超时值。

修改参数会撤销该方案已有的 MCP 授权，这是 MCDH 的正常安全行为。

## 显式参数与命令行

默认从 `MCDH_INPUT_DIR`、`MCDH_OUTPUT_DIR` 读取路径。也可填写以下四条参数测试占位符：

~~~text
--input
{input_dir}
--output-dir
{output_dir}
~~~

直接从终端调用时必须指定已有的输入目录和一个已有的空输出目录。输出目录不得位于输入目录内部；打包器拒绝覆盖已有文件。例如：

~~~powershell
& 'D:\Tools\MCDH-TestPacker.exe' --input 'D:\MyProject' --output-dir 'D:\EmptyOutput' --mode success
~~~

`--help` 显示简要英文用法。配置错误退出码为 `2`，刻意失败模式退出码为 `23`；其余模式的程序退出码为 `0`，再由 MCDH 判断产物是否有效。

## 重新编译

源码位于 `crates/mcdh-core/examples/test_exporter.rs`，使用仓库已有的 Rust 依赖：

~~~powershell
cargo test -p mcdh-core --example test_exporter
cargo build -p mcdh-core --example test_exporter --release --locked
~~~

原始编译输出为 `target/release/examples/test_exporter.exe`。用户测试目录为 `release/test-packer`，其中 EXE 与说明文件可一起移动；不要把测试工具放在待导出的项目目录内。
