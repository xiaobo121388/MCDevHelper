# MCDH · MCDevHelper

MCDH 是面向网易《我的世界》中国版 PE 创作者的 Windows 本地优先组件管理器。它可以统一管理 AddOn、地图和 Material/Light 材质组件，兼容 MCStudio（MCS）现有作品，并通过独立的 MCP 服务让 AI 使用同一套核心功能。

## 特点

- 本地优先：无账号、无遥测、无在线字体、无 CDN，也不监听网络端口；仅在用户主动检查更新时访问 GitHub。
- 轻量桌面端：Tauri 2 + 系统 WebView2，简体中文界面，可跟随系统或固定为浅色/深色主题。
- 多来源管理：首次启动自动发现所有逻辑盘的 MCS 工作目录，随后只扫描已保存来源，也可手动重新发现或添加自定义 MCS 路径。
- 完整工作流：新建、导入、复制、移动、导出、双重确认删除、标签、UUID 重生、版本提升、目录和 VS Code 打开。
- 快速查找：按标签筛选，并按 MCS 时间、名称、修改日期、创建日期或大小排序；默认按修改日期倒序。
- 安全导入：支持文件夹、ZIP、mcpack、mcaddon 和内嵌包，拒绝路径穿越、绝对路径与符号链接条目。
- JSONC 兼容：组件文件、MCS 配置、世界包清单、内置模板和本地 JSON 设置均支持 `//`、`/* ... */` 注释与尾随逗号。
- MCS 兼容：识别 Type 1/3/4/7；可配置本地开发者身份和命名空间，并生成兼容的 `studio.json` 与 `work.mcscfg`。
- AI 接口：`mcdh-mcp.exe` 使用标准输入输出，提供 21 个严格 JSON Schema 工具，不提供删除组件工具。

## 系统要求

- Windows 10/11 x64。
- 已安装系统 WebView2 Runtime。Windows 10 的受支持版本和 Windows 11 通常已随系统提供；MCDH 不联网下载运行时。
- 安装和管理组件不需要管理员权限，也不需要 Node.js、Rust 或网络连接；“检查更新”和打开反馈页面需要网络。

发行包当前未进行商业代码签名，首次运行时 Windows 可能显示 SmartScreen 提示。请核对 `SHA256SUMS.txt` 后再运行。

## 安装与便携版

- 安装版：运行 `MCDH-<版本>-windows-x64-setup.exe`，默认安装到当前用户的 `%LOCALAPPDATA%`，不会请求管理员权限。
- 便携版：解压 `MCDH-<版本>-windows-x64-portable.zip`，保持 `MCDH.exe` 与 `mcdh-mcp.exe` 位于同一目录，然后运行 `MCDH.exe`。

两种版本都会把索引数据库保存到 `%LOCALAPPDATA%\MCDH\mcdh.db`。组件文件始终保存在用户选择的位置；移除来源只删除索引登记，不删除磁盘内容。

## 快速使用

1. 首次启动且没有保存记录时，MCDH 自动扫描 `<盘符>:\MCStudioDownload\work\<账号>\Cpp\AddOn|Map|Material|Light` 并保存找到的分类目录。
2. 打开左下角“设置”管理路径。“添加组件库”扫描所选目录的直接子目录，“添加单个组件”只管理所选目录；也可添加任意 MCS 分类目录或主动重新扫描逻辑盘。
3. 设置面板左侧按“路径管理、MCS 身份、外观、开发工具、关于”分类；可配置新建默认目录、开发者身份和跟随系统/亮色/暗色主题。
4. 使用“新建组件”从已配置目录的下拉框选择目标；启用“MCS 兼容配置”后可填写命名空间，默认是 `mcdh`。
5. 组件卡片右下角可打开目录、用 VS Code 打开或进入配置面板；删除需要连续两次确认并会永久移除整个组件目录。
6. 导出始终生成清洁 ZIP：AddOn 根目录只保留检测到的 BP/RP，并递归剔除 `.pyi`、`.pyc`；地图和材质移除点号项及 MCS 私有配置。大型 AddOn 会在后台直接压缩已识别包，界面显示“导出中…”且不会重新扫描全部来源。

主界面出现扫描问题提示时可直接打开详情，逐条查看路径和原因，并选择打开最近可访问的文件夹、移除 MCDH 来源记录或忽略。移除来源和忽略都不会删除磁盘文件；已忽略问题可从筛选栏重新显示。

复制组件时可选择保留或重生 manifest UUID；复制到 MCS 时总会生成新的 MCS UID。移动默认保留 manifest UUID。重要作品建议先自行备份。

UUID 重生、版本提升和标签同步会在原 JSONC 文本中定点更新并原子写回，保留已有注释、缩进、尾随逗号和 UTF-8 BOM。UUID 与版本快捷操作通过本地索引直接定位单个组件，不会额外扫描全部来源；MCP 的 JSON-RPC 消息仍须使用标准 JSON。

## 检查更新与反馈

打开“设置 > 关于”可以查看当前版本。MCDH 不会在启动或后台自动检查；只有点击“检查更新”后，才会向 GitHub 官方 [`GET /repos/xiaobo121388/MCDevHelper/releases/latest`](https://docs.github.com/en/rest/releases/releases?apiVersion=2026-03-10#get-the-latest-release) 接口发起一次未认证请求，并展示最新正式 Release，不会自动下载或安装。未认证请求只能读取公开资源；仓库未公开或尚无正式 Release 时会显示“未找到公开 Release”。

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

`list_components`、`get_component`、`refresh_components`、`list_sources`、`add_single_component`、`add_library`、`add_mcs_path`、`rescan_mcs_paths`、`remove_source`、`get_settings`、`set_settings`、`create_component`、`import_component`、`copy_component`、`move_component`、`export_component`、`set_component_tags`、`regenerate_manifest_uuids`、`bump_manifest_version`、`open_component_directory`、`open_component_in_vscode`。

## 开发与验证

需要 Node.js/pnpm、Rust stable MSVC 工具链和 Visual Studio C++ Build Tools：

```powershell
pnpm install --frozen-lockfile
pnpm test
pnpm build
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

生成 Windows 安装版、便携版和 SHA-256 清单：

```powershell
pnpm release:windows
```

输出位于 `release\`。构建脚本会先生成第三方许可清单，再编译 `mcdh-mcp.exe` sidecar，最后构建当前用户 NSIS 安装包和便携 ZIP。

## 数据与隐私

- SQLite 使用 WAL、5 秒 busy timeout 和跨进程文件锁。
- MCS 模板源码仅包含 `mcdh@local.invalid`、`MCDH`、`0` 等中性默认值；用户可在设置中替换这些本地生成信息，模板除 MCS 必需的实际目标路径外不含本机绝对路径。
- 应用没有账号系统、游戏启动/测试功能、遥测或自动更新。主动检查更新时仅请求 `api.github.com` 的公开 Release 元数据，反馈则交由系统浏览器打开 GitHub；其他组件管理功能不联网。
- 设置环境变量 `MCDH_DATA_DIR` 可为自动化测试隔离数据库；设置 `MCDH_DISABLE_MCS_SCAN=1` 可在测试进程中禁用自动 MCS 扫描。

## 开源参考与许可

架构使用 Tauri 2、React、TypeScript、Vite、Tailwind CSS、rusqlite、zip-rs、uuid-rs 与官方 Rust MCP SDK。组件识别思路参考 MCDevTool 和 BDSAddonManager 的公开设计，但未复制其源码。

MCDH 源代码采用 [MIT License](LICENSE) 开源。第三方包仍分别遵循其自身许可证，完整声明见 [THIRD_PARTY_LICENSES.md](THIRD_PARTY_LICENSES.md)。
