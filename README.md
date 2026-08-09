# MCDH · MCDevHelper

MCDH 是面向网易《我的世界》中国版 PE 创作者的 Windows 离线组件管理器。它可以统一管理 AddOn、地图和 Material/Light 材质组件，兼容 MCStudio（MCS）现有作品，并通过独立的 MCP 服务让 AI 使用同一套核心功能。

## 特点

- 完全离线：无账号、无遥测、无更新检查、无在线字体、无 CDN，也不监听网络端口。
- 轻量桌面端：Tauri 2 + 系统 WebView2，简体中文界面，自动跟随系统浅色/深色主题。
- 多来源管理：自动扫描所有逻辑盘的 MCS 工作目录，可另加单个组件目录或组件库目录。
- 完整工作流：新建、导入、复制、移动、导出、标签、UUID 重生、版本提升、目录和 VS Code 打开。
- 安全导入：支持文件夹、ZIP、mcpack、mcaddon 和内嵌包，拒绝路径穿越、绝对路径与符号链接条目。
- MCS 兼容：识别 Type 1/3/4/7；需要时生成中性身份的 `studio.json` 和 `work.mcscfg`。
- AI 接口：`mcdh-mcp.exe` 使用标准输入输出，提供 17 个严格 JSON Schema 工具，不提供删除组件工具。

## 系统要求

- Windows 10/11 x64。
- 已安装系统 WebView2 Runtime。Windows 10 的受支持版本和 Windows 11 通常已随系统提供；MCDH 不联网下载运行时。
- 安装和使用不需要管理员权限，也不需要 Node.js、Rust 或网络连接。

发行包当前未进行商业代码签名，首次运行时 Windows 可能显示 SmartScreen 提示。请核对 `SHA256SUMS.txt` 后再运行。

## 安装与便携版

- 安装版：运行 `MCDH-<版本>-windows-x64-setup.exe`，默认安装到当前用户的 `%LOCALAPPDATA%`，不会请求管理员权限。
- 便携版：解压 `MCDH-<版本>-windows-x64-portable.zip`，保持 `MCDH.exe` 与 `mcdh-mcp.exe` 位于同一目录，然后运行 `MCDH.exe`。

两种版本都会把索引数据库保存到 `%LOCALAPPDATA%\MCDH\mcdh.db`。组件文件始终保存在用户选择的位置；移除来源只删除索引登记，不删除磁盘内容。

## 快速使用

1. 打开“路径管理”。MCDH 会自动扫描 `<盘符>:\MCStudioDownload\work\<账号>\Cpp\AddOn|Map|Material|Light`。
2. “添加组件库”会扫描所选目录的直接子目录；“添加单个组件”只管理所选目录。
3. 使用“新建组件”选择模组、材质或地图及目标目录。只有目标确实是 MCS 分类目录时才启用“MCS 兼容配置”。
4. 组件卡片右下角可打开目录、用 VS Code 打开，或进入配置面板。
5. 导出始终生成清洁 ZIP：AddOn 根目录只保留检测到的 BP/RP，地图和材质移除点号项及 MCS 私有配置。

复制组件时可选择保留或重生 manifest UUID；复制到 MCS 时总会生成新的 MCS UID。移动默认保留 manifest UUID。重要作品建议先自行备份。

## MCP 配置

在“路径管理”底部点击“复制客户端配置”，或手动配置：

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

`list_components`、`get_component`、`refresh_components`、`list_sources`、`add_single_component`、`add_library`、`remove_source`、`create_component`、`import_component`、`copy_component`、`move_component`、`export_component`、`set_component_tags`、`regenerate_manifest_uuids`、`bump_manifest_version`、`open_component_directory`、`open_component_in_vscode`。

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
- MCS 模板仅使用 `mcdh@local.invalid`、`MCDH`、`0` 等中性身份，除 MCS 必需的实际目标路径外不含本机绝对路径。
- 应用没有账号系统、游戏启动/测试功能、遥测、网络请求或自动更新。
- 设置环境变量 `MCDH_DATA_DIR` 可为自动化测试隔离数据库；设置 `MCDH_DISABLE_MCS_SCAN=1` 可在测试进程中禁用自动 MCS 扫描。

## 开源参考与许可

架构使用 Tauri 2、React、TypeScript、Vite、Tailwind CSS、rusqlite、zip-rs、uuid-rs 与官方 Rust MCP SDK。组件识别思路参考 MCDevTool 和 BDSAddonManager 的公开设计，但未复制其源码。

第三方包及声明许可见 [THIRD_PARTY_LICENSES.md](THIRD_PARTY_LICENSES.md)。本仓库自身当前标记为 `UNLICENSED`；除第三方组件各自许可授予的权利外，未另行授予 MCDH 源代码使用许可。
