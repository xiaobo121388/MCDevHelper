# MCDH · MCDevHelper

MCDH 是面向网易《我的世界》中国版 PE 创作者的 Windows 本地优先组件管理器。它可以统一管理 AddOn、地图和 Material/Light 材质组件，兼容 MCStudio（MCS）现有作品，并通过独立的 MCP 服务让 AI 使用同一套核心功能。

## 特点

- 本地优先：无账号、无遥测、无在线字体、无 CDN，也不监听网络端口；启动时访问 GitHub 检查正式更新，默认自动下载校验后的 MCDK 更新。
- 轻量桌面端：Tauri 2 + 系统 WebView2，简体中文界面，可跟随系统或固定为浅色/深色主题。
- 多来源管理：首次启动自动发现所有逻辑盘的 MCS 工作目录，随后只扫描已保存来源，也可手动重新发现或添加自定义 MCS 路径。
- 完整工作流：新建、导入、复制、移动、导出、双重确认删除、标签、UUID 重生、版本提升、目录和 VS Code 打开。
- 游戏启动：内置 MCDK，卡片右下角播放按钮启动当前组件的独立控制台，首次配置由 MCDK 引导。
- 可携带元数据：组件根目录的 `.mcdh.json` 保存显示名称、标签和收藏状态，左侧收藏视图可快速筛选常用组件。
- 快速查找：按标签筛选，并按 MCS 时间、名称、修改日期、创建日期或大小排序；默认按修改日期倒序。
- 安全导入：支持文件夹、ZIP、mcpack、mcaddon 和内嵌包，拒绝路径穿越、绝对路径与符号链接条目。
- JSONC 兼容：组件文件、MCS 配置、世界包清单、内置模板和本地 JSON 设置均支持 `//`、`/* ... */` 注释与尾随逗号。
- MCS 兼容：识别 Type 1/3/4/7；可配置本地开发者身份和命名空间，并生成兼容的 `studio.json` 与 `work.mcscfg`。
- AI 接口：`mcdh-mcp.exe` 使用标准输入输出，提供 22 个严格 JSON Schema 工具，不提供删除组件工具。

## 系统要求

- Windows 10/11 x64。
- 已安装系统 WebView2 Runtime。Windows 10 的受支持版本和 Windows 11 通常已随系统提供；MCDH 不联网下载运行时。
- 安装和管理组件不需要管理员权限，也不需要 Node.js、Rust 或持续网络连接；启动更新检查和打开反馈页面需要网络，检查失败不影响本地功能。

发行包当前未进行商业代码签名，首次运行时 Windows 可能显示 SmartScreen 提示。请核对 `SHA256SUMS.txt` 后再运行。

## 安装与便携版

- 安装版：运行 `MCDH-<版本>-windows-x64-setup.exe`，默认安装到当前用户的 `%LOCALAPPDATA%`，不会请求管理员权限。
- 便携版：解压 `MCDH-<版本>-windows-x64-portable.zip`，保留 `MCDH.exe`、`mcdh-mcp.exe` 和 `mcdk` 资源目录，然后运行 `MCDH.exe`。

两种版本都会把索引数据库保存到 `%LOCALAPPDATA%\MCDH\mcdh.db`。组件文件始终保存在用户选择的位置；移除来源只删除索引登记，不删除磁盘内容。

## 快速使用

1. 首次启动且没有保存记录时，MCDH 自动扫描 `<盘符>:\MCStudioDownload\work\<账号>\Cpp\AddOn|Map|Material|Light` 并保存找到的分类目录。
2. 打开左下角“设置”管理路径。“添加组件库”扫描所选目录的直接子目录，“添加单个组件”只管理所选目录；也可添加任意 MCS 分类目录或主动重新扫描逻辑盘。
3. 设置面板左侧按“路径管理、MCS 身份、外观、开发工具、关于”分类；可配置新建默认目录、开发者身份和跟随系统/亮色/暗色主题。
4. 使用“新建组件”从已配置目录的下拉框选择目标；启用“MCS 兼容配置”后可填写命名空间，默认是 `mcdh`。
5. 组件卡片可一键收藏，右下角可打开目录、用 VS Code 打开或进入配置面板；配置面板可修改显示名称、标签和收藏状态。删除需要连续两次确认并会永久移除整个组件目录。
6. “导出游戏 ZIP”继续生成清洁游戏包：AddOn 根目录只保留检测到的 BP/RP，并递归剔除 `.pyi`、`.pyc`；地图和材质移除点号项、`.mcdh.json` 及 MCS 私有配置。“导出完整 ZIP”保留组件根目录内的全部普通文件和空目录，适合备份和迁移编辑环境。成功导出后会记住目录，下次自动填写；遇到同名 ZIP 时可选择覆盖原文件或追加序号。
7. 导入默认按游戏内容清洁处理；启用“完整恢复”后保留点号项、MCS 配置和开发辅助文件。两种导入仍会拒绝路径穿越、绝对路径和符号链接。

主界面出现扫描问题提示时可直接打开详情，逐条查看路径和原因，并选择打开最近可访问的文件夹、移除 MCDH 来源记录或忽略。移除来源和忽略都不会删除磁盘文件；已忽略问题可从筛选栏重新显示。

复制组件时可选择保留或重生 manifest UUID；复制到 MCS 时总会生成新的 MCS UID。移动默认保留 manifest UUID。重要作品建议先自行备份。

UUID 重生、版本提升和标签同步会在原 JSONC 文本中定点更新并原子写回，保留已有注释、缩进、尾随逗号和 UTF-8 BOM。UUID 与版本快捷操作通过本地索引直接定位单个组件，不会额外扫描全部来源；MCP 的 JSON-RPC 消息仍须使用标准 JSON。

## 组件元数据

MCDH 新建、导入或复制组件时会在根目录生成 `.mcdh.json`；没有该文件的旧组件仍可正常使用，只有在第一次修改显示名称、标签或收藏时才会创建。文件格式如下，读取时兼容 JSONC 注释和尾随逗号：

```json
{
  "schema_version": 1,
  "display_name": "组件名称",
  "tags": ["开发", "测试"],
  "favorite": false
}
```

有效配置优先于 MCS、manifest 和本机旧标签记录。配置损坏或版本不受支持时，组件仍会使用原始信息显示，同时在扫描问题中报告 `.mcdh.json`；MCDH 不会静默覆盖损坏配置。完整导出会携带该文件；旧组件缺少配置时只在完整 ZIP 内补入生成的配置，不修改源目录。符号链接不会被复制或导出。

## 检查更新与反馈

### MCDK 游戏启动与自动更新

卡片右下角操作顺序为“启动游戏、打开目录、VS Code、配置”。点击播放按钮后，以该组件根目录为工作目录启动内置 MCDK，不需要配置系统 PATH。首次没有 `.mcdev.json` 时，在独立控制台选择游戏程序并完成 MCDK 引导；已有配置不被 MCDH 改写。游戏本体需自行安装。

同一 MCDH 数据目录一次管理一个 MCDK 会话，关闭 MCDH 后游戏继续运行，重开应用会校验并恢复会话状态。运行中的组件不能通过 MCDH 删除、移动、重生 UUID 或提升清单版本；MCS 配置写入也会被阻止。世界复用、包加载和地图部署遵循项目的 MCDK 配置，本期没有多开、内置日志或 MCP 调试面板。

MCDK 默认在每次 MCDH 启动时后台检查正式 Release，有新版就自动下载，无需确认。在“设置 > 开发工具 > MCDK”可查看版本、更新状态和失败原因，关闭“自动更新”或手动“检查更新”。关闭自动更新后，手动检查发现新版需点击“立即更新”；开关立即保存，不依赖“保存设置”。

更新使用独立版本目录，校验官方资产摘要、大小及 Windows x64 PE 格式后才切换，不覆盖运行中的程序。网络或校验失败保留原版本，文件损坏时优先回退上一版，再使用内置副本。自动下载期间关闭开关会取消尚未激活的更新，已完成更新不会回退。默认资源目录为 `%LOCALAPPDATA%\MCDH\tools\mcdk`，版本元数据及开关独立存入本地数据库。

### MCDH 本体更新

MCDH 每次启动会向 GitHub 官方 [`GET /repos/xiaobo121388/MCDevHelper/releases/latest`](https://docs.github.com/en/rest/releases/releases?apiVersion=2026-03-10#get-the-latest-release) 接口发起一次未认证请求；发现新版本时弹窗展示 Release 名称、更新说明和官方下载入口，但不会自动下载或安装。网络不可用时静默跳过，不影响组件管理；也可以在“设置 > 关于”手动重新检查。

应用会在本机记录上次启动的版本。首次安装或检测到版本升级后的第一次启动会优先显示该版本的内置更新日志，并立即记录为已读；同一次启动若还发现更高版本，会在关闭更新日志后继续显示更新提示。

“反馈问题”会使用系统默认浏览器打开仓库的 GitHub 新建 Issue 页面，MCDH 不会代替用户填写或提交内容。

## MCP 配置

在“设置 > 开发工具”点击“复制客户端配置”，或手动配置：

```json
{
  "mcpServers": {
    "mcdh": {
      "command": "C:\\完整路径\\mcdh-mcp.exe"
    }
  }
}
```

MCP 仅使用 stdio；stdout 只输出协议消息，运行日志写入 stderr。可用工具：

`list_components`、`get_component`、`refresh_components`、`list_sources`、`add_single_component`、`add_library`、`add_mcs_path`、`rescan_mcs_paths`、`remove_source`、`get_settings`、`set_settings`、`create_component`、`import_component`、`copy_component`、`move_component`、`export_component`、`set_component_tags`、`set_component_metadata`、`regenerate_manifest_uuids`、`bump_manifest_version`、`open_component_directory`、`open_component_in_vscode`。`import_component` 和 `export_component` 的 `content_mode` 可选 `clean` 或 `full`，省略时保持 `clean`。`export_component.conflict_policy` 可选 `rename`（默认追加序号）、`overwrite` 或 `error`。

## 开发与验证

需要 Node.js/pnpm、Rust stable MSVC 工具链和 Visual Studio C++ Build Tools：

```powershell
pnpm install --frozen-lockfile
pnpm mcdk:prepare
pnpm test
pnpm build
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

生成 Windows 安装版、便携版和 SHA-256 清单：

```powershell
pnpm release:windows
```

输出位于 `release\`。构建脚本会先生成第三方许可清单，下载并验证 `assets/mcdk/bundled.json` 锁定的 MCDK 资源，再编译 `mcdh-mcp.exe` sidecar，最后构建当前用户 NSIS 安装包和便携 ZIP。MCDK 二进制位于被 Git 忽略的生成目录，离线重新构建可复用已验证的缓存。开发模式也应先运行 `pnpm mcdk:prepare`。

MCDK 原生控制台、离线资源验证命令及人工验收清单见 [MCDK 验证说明](docs/mcdk-validation.md)。

## 数据与隐私

- SQLite 使用 WAL、5 秒 busy timeout 和跨进程文件锁。
- MCS 模板源码仅包含 `mcdh@local.invalid`、`MCDH`、`0` 等中性默认值；用户可在设置中替换这些本地生成信息，模板除 MCS 必需的实际目标路径外不含本机绝对路径。
- 应用没有账号系统或遥测。MCDH 本体更新仅提示下载入口；MCDK 自动更新请求 `api.github.com` 的公开 Release 元数据，从 `github.com` 及其官方 Release 资产域名下载程序。不会上传组件源码、游戏日志或目录内容。MCDK 的游戏网络与可选调试服务由其自身及项目配置管理。
- 设置环境变量 `MCDH_DATA_DIR` 可为自动化测试隔离数据库；设置 `MCDH_DISABLE_MCS_SCAN=1` 可在测试进程中禁用自动 MCS 扫描。

## 开源参考与许可

架构使用 Tauri 2、React、TypeScript、Vite、Tailwind CSS、rusqlite、zip-rs、uuid-rs 与官方 Rust MCP SDK。组件识别思路参考 MCDevTool 和 BDSAddonManager 的公开设计，但未复制其源码。

MCDH 源代码采用 [MIT License](LICENSE) 开源。第三方包仍分别遵循其自身许可证，完整声明见 [THIRD_PARTY_LICENSES.md](THIRD_PARTY_LICENSES.md)。
