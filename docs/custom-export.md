# 自定义导出接入协议 v1

自定义导出是独立入口，不会改变“导出游戏 ZIP”和“导出完整 ZIP”的内容规则。在“设置 > 自定义导出”新增方案、填写程序和参数，保存后，启用且适用于当前组件类型的方案会以按钮形式出现在两个内置 ZIP 按钮旁。

## 最小接入

仓库的 `examples/custom-export/pack.py` 是仅依赖 Python 3 标准库的可运行示例。它将输入目录的普通内容打包为 ZIP，保留空目录，不执行内置游戏包清洁规则。

例如程序选择 `C:\Python312\python.exe`，逐条填写参数（实际界面不需要外围引号）：

~~~json
[
  "-u",
  "D:/Documents/ChatGPT/MCDevHelper/examples/custom-export/pack.py",
  "--input",
  "{input_dir}",
  "--output-dir",
  "{output_dir}",
  "--name",
  "release.zip"
]
~~~

也可以只传 `-u` 和脚本路径，示例会从 `MCDH_INPUT_DIR` 与 `MCDH_OUTPUT_DIR` 读取路径。`-u` 使 Python 日志及时刷新。已有 EXE 只要能接受输入目录、输出目录参数，或读取下面的环境变量，就不需要额外包装；如果原程序只能输出到固定位置，请编写适配脚本把最终文件移入指定产物目录。

## 输入与参数

| 参数占位符 | 环境变量 | 含义 |
|---|---|---|
| `{input_dir}` | `MCDH_INPUT_DIR` | 方案选择的完整副本或原始组件目录 |
| `{output_dir}` | `MCDH_OUTPUT_DIR` | 初始为空的最终产物目录 |
| `{work_dir}` | `MCDH_WORK_DIR` | 本次任务的中间文件目录 |
| `{component_id}` | `MCDH_COMPONENT_ID` | 组件稳定 ID |
| `{component_name}` | `MCDH_COMPONENT_NAME` | 组件显示名称，不保证适合作为文件名 |
| `{component_kind}` | `MCDH_COMPONENT_KIND` | `addon`、`map` 或 `material` |

另提供 `MCDH_EXPORT_PROTOCOL_VERSION=1`。参数按数组逐条、单次替换，不对替换出的组件名称或路径再做模板展开。未知占位符保持原文。不要给单个参数额外套 shell 引号；MCDH 保留空参数、空格、引号和反斜杠的参数边界，不经过 shell 展开。

程序必须为本机 EXE 绝对路径；脚本由 Python、Node、PowerShell 等明确的解释器执行。MCDH 不自动运行 `.bat/.cmd`、拼接命令行或配置 shell。PowerShell 脚本使用 `powershell.exe` 或 `pwsh.exe`，参数以 `-NoProfile`、`-NonInteractive`、`-File` 和脚本绝对路径开头；不要用 `-Command` 拼接包含项目名称或路径的代码。

进程默认工作目录为 `input_dir`，也可指定已有绝对目录；它与 `work_dir` 不同。除上述变量外继承宿主进程环境，因此不要把不可信程序视为受限运行环境。

## 产物与日志

- 程序自行命名和选择格式。`output_dir` 根目录必须恰好包含一个非空普通文件，不能包含子目录、额外报告或链接；中间文件放在 `work_dir`。
- 只有退出码 `0` 且产物检查通过才发布。MCDH 不根据扩展名检查 ZIP、游戏包或加密包结构，也不保证自定义产物能够被游戏识别。
- 文件名必须符合 Windows 文件名规则。MCDH 使用原名保存，不替程序生成文件名；重名时选择取消、追加序号或覆盖。追加序号位于最后一个扩展名前，例如 `bundle.tar (2).gz`。
- 重名选择复用现有产物，不重新打包。等待超过 30 分钟自动失败并清理；成功、失败及取消记录保留 30 分钟。
- stdout、stderr 都是日志，不需要输出 JSON；stdout 不作为产物清单。日志默认 UTF-8，可在方案中选择 GB18030。两条流各自保持顺序，跨流交错以宿主收到的顺序为准。
- 大量输出不会堵住程序；宿主持续读取，仅保留最近 1 MiB。界面和 API 会报告截断。Python 建议 `-u`；PowerShell 建议显式设置 `[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false)`。
- 超时默认 1800 秒，计入准备输入和程序执行时间；重名等待另计。取消、超时和宿主退出会终止本任务的进程树，已成功提交的文件不撤销。

## 文件与权限边界

完整副本是默认方式：复制普通文件和空目录，保留点号文件及已有 `.mcdh.json`，跳过符号链接和目录联接，不自动生成元数据，也不应用内置游戏包过滤。程序可在副本中预处理，MCDH 不把改动同步回原目录。

原目录模式不复制。保存此配置会要求确认风险，程序对项目文件的修改或删除无法撤销；组件正在 MCDK 中运行时拒绝使用原目录模式。两种模式的导出目录都不能位于源组件内。

**临时副本不是安全沙箱。外部程序拥有当前 Windows 用户权限，仍可访问源项目、网络和其他目录。仅运行可信程序。** Job Object 只管理本次进程树，不隔离文件访问。MCDH 的修改锁仅协调自身桌面及 MCP 操作，不能阻止编辑器、游戏或其他工具改动文件。

方案仅保存到本机数据库的独立版本化设置项，不写进组件元数据，不随组件导入自动启用。修改程序、参数、输入模式或工作目录会撤销原 MCP 授权；保存新执行配置后，再单独勾选“允许 MCP 执行”并保存。MCP 不能修改方案或授权。授权绑定配置而非程序文件摘要，更新或替换同一路径的程序、脚本仍需用户自行确认可信性。

## MCP 调用

1. `list_custom_export_profiles {}`：仅返回启用且已授权方案。
2. `start_custom_export {"component_id":"...","profile_id":"...","destination":"D:/Exports"}`：立即返回含任务 ID 的状态。可传 `conflict_policy` 为 `error`（默认等待选择）、`rename` 或 `overwrite`。
3. `get_custom_export_task {"task_id":"...","cursor":0}`：返回状态、日志和 `next_cursor`；下一次用该游标避免重复日志，建议 500ms 或更慢查询。
4. `awaiting_conflict` 时调用 `resolve_custom_export_conflict {"task_id":"...","conflict_policy":"rename"}`，也可选择 `overwrite`；放弃用 `cancel_custom_export`。
5. `cancel_custom_export {"task_id":"..."}`：请求取消，继续查询至 `cancelled`。已成功提交时保持 `succeeded`。

状态为 `preparing`、`running`、`validating`、`awaiting_conflict`、`publishing`、`succeeded`、`failed`、`cancelled`。成功任务的 `result` 使用现有 `OperationResult`；失败任务的 `error` 含错误码、消息及可用的退出码，日志仍在 `logs` 中。

任务属于启动它的宿主进程，桌面和 MCP 不互相接管。同一桌面宿主中关闭再打开组件面板可以恢复查看；重启宿主不能恢复旧任务。异常退出遗留工作区仅在其宿主独占租约已释放后清理，仍在运行的其他宿主目录不会被删除。

常见错误码：`not_authorized`、`profile_not_found`、`profile_disabled`、`incompatible_profile`、`start_failed`、`execution_failed`、`timed_out`、`cancelled`、`invalid_artifact`、`publish_failed`、`conflict_expired`、`task_not_found`。已有内置导出的 `destination_exists` 行为不变；自定义导出通过 `awaiting_conflict` 与 `conflict_path` 表示同名等待。
