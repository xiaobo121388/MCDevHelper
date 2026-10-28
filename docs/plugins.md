# MCDH GitHub 插件

这是 **MCDH 应用插件 API 1**，不是 MCDK / MCDE 的原生插件接口，也不直接兼容 DeepSeek Harness、Codex 或其他应用的插件。内置组件管理和普通游戏启动不依赖此系统；MCDE 通过下述内置原生适配器注册。

## 内置 MCDE

桌面启动时将 MCDE 注册为 `mcdh.mcde`，在插件页提供启用开关、原生组件目录设置、宿主修订和错误状态。目录配置复用原有 MCDE 服务和已保存配置；开发工具设置中的入口也保留。组件插件菜单提供“在 MCDE 中打开”，原有快捷按钮同样受插件启用状态控制，后端每次启动重新检查，不能绕过禁用。

这是随 MCDH 提供的原生适配器，不在 QuickJS 中加载 MCDE 的 EXE/DLL。MCDK 原生插件 ID `com.xiaobo.mcdeveditor`、ABI 校验、独立宿主、项目配置保护和会话锁保持不变。列表版本为 MCDH 适配器版本，目录设置区域显示原生 MCDE 版本。适配器的内容指纹标识编译内置清单和锁定宿主修订，不宣称是原生目录全部文件的摘要。

MCDE 随应用更新，可停用但不能从插件页卸载或被本地 / GitHub 包替换；停用状态跨重启保留。正在启动或已有 MCDE 会话时拒绝修改启用状态。此适配器只提供桌面命令，不注册 MCP 工具，也不允许授权 MCP。它属于应用内置功能，不使用第三方包的信任确认流程。

## 安装与管理

1. 在主侧栏打开“插件”，输入 `owner/repo`、`github:owner/repo` 或 `https://github.com/owner/repo`，点击“添加插件”。只支持公开 GitHub 仓库，不读取本机 Git 凭据。
2. 查看预览中的名称、作者声明、来源、版本、Release 标签或提交、兼容范围和内容指纹。名称和作者来自插件自己的声明，并非 GitHub 实名或安全认证。
3. 阅读全权限警告，勾选信任并确认安装。预览、校验、安装和列举工具本身均不执行后台插件代码；只有确认后的命令调用或面板打开才加载代码。
4. 在插件列表启用、停用、编辑设置、查看错误日志或“检查更新”。检查更新只产生新预览，必须再次确认才安装，不自动更新插件。
5. 卸载默认保留数据和设置。同一来源重新安装时恢复符合新设置 Schema 的设置；不同来源不会恢复保留的设置。只有明确勾选“同时清除插件数据”才删除数据目录。

普通用户不需要 Node.js、Git、Rust 或编译环境。普通源码仓库不能直接当插件安装：仓库根目录必须有 `mcdh-plugin.json` 及其声明的可运行入口，所有依赖必须预先打包。

### 本地导入

“本地导入”选择插件 ZIP；旁边的文件夹按钮选择插件目录。ZIP 和所选目录的根部都必须直接包含 `mcdh-plugin.json` 以及已构建入口，不执行依赖安装或构建脚本。导入前同样需要预览、确认全权限信任。

导入器先创建独立快照，再进行校验；不会在原始目录中执行或修改文件。ZIP 上限 128 MiB，解压或目录快照上限 512 MiB、10,000 个条目，拒绝链接、重解析点和不安全路径。预览后源文件发生变化不会改变已经预览的内容。目录来源的摘要是文件摘要清单的指纹，不是 ZIP 摘要。

本地插件的“检查更新”会重新打开相应文件或目录选择器，可以选择新版 ZIP 或目录，但插件 ID 必须一致；内容变化仍会撤销 MCP 授权。所有本地导入属于本地来源，记录原路径，不伪造 GitHub 仓库。本地来源与 GitHub 来源不能直接覆盖同 ID 插件，切换来源须先显式卸载。卸载不删除原始 ZIP 或目录。

### 下载规则

- 先查询最新正式 Release 的固定资产 `mcdh-plugin.zip`。清单 `version` 必须与 Release 标签一致，标签可带 `v` 前缀，例如 `v1.0.0`。
- 存在该资产时，必须通过 GitHub API 提供的 `sha256:` 摘要及字节大小校验；缺少摘要、下载失败、ZIP 无效或插件清单不合规时直接报错，不改装分支代码。
- 没有正式 Release 或该 Release 没有插件资产时，先解析默认分支到一个确定的 40 位提交 SHA，再下载该提交的归档。GitHub 外层目录由安装器移除，插件仍须位于仓库根目录。
- 记录下载摘要、逐文件 SHA-256 清单及由文件清单生成的内容指纹。分支安装额外记录提交 SHA。摘要仅证明内容一致，不代表代码可信、经过审核或已签名。
- 下载最多 128 MiB，展开最多 512 MiB，最多 10,000 个条目（包括隐式父目录）。拒绝路径穿越、绝对路径、链接、重解析点、Windows 设备名、备用数据流和大小写冲突。
- 在临时目录完整校验后发布到独立版本目录，再切换 SQLite 注册记录。失败保留原记录；中断下载不改动已安装版本。相同 ID 不能覆盖来自其他仓库的已安装插件。

## 权限边界

**插件以当前 Windows 用户的完整权限运行，没有操作系统沙箱。** 它可以通过 SDK 读写文件、联网、执行本机程序，也可能删除文件或绕过内置 MCP 工具的功能限制。只安装和授权自己信任的代码。

QuickJS 的内存限制、命令超时、归档检查和受限资源协议是稳定性与完整性措施，不是恶意代码沙箱。前端 Shadow DOM 仅隔离样式，插件与应用共用 WebView，不隔离 DOM、应用权限或机密。停用可以阻止后续宿主调用，但不能撤销插件已经执行的外部动作。

## 文件结构与发布

一个仓库或发布 ZIP 的根目录对应一个插件：

```text
mcdh-plugin.json
main.js
panel.js
logo.png
lib/
  bundled-helper.js
```

后台和面板都是 ES modules。可以用纯 JavaScript 编写，也可在作者自己的构建环境将 TypeScript 和第三方依赖编译、打包后发布。宿主不会执行 `npm install`、生命周期脚本、构建脚本或 Git 命令；不会替作者安装解释器和外部 EXE。

正式发布时将上述文件直接放到 `mcdh-plugin.zip` 根目录，不能多套一个插件文件夹，再上传到正式 Release。没有 Release 也可以把已构建文件提交到默认分支。

完整示例在 [examples/plugins/hello](../examples/plugins/hello)。发布此示例时，把该目录的**内容**放到自己的插件仓库根目录或 ZIP 根目录；本项目仓库根目录不是一个可安装的插件。示例中的 `$schema` 是本仓库内的编辑器引用，独立发布时可以删除或调整，它不会触发宿主下载。

## 清单

机器可读 Schema：[plugin-manifest.schema.json](plugin-manifest.schema.json)。Rust 清单结构生成该文件，安装器另外验证 ID、相对路径、SemVer、声明引用和 JSON Schema 默认值等语义条件。清单不接受未定义字段。

```json
{
  "schema_version": 1,
  "api_version": 1,
  "id": "example.hello",
  "name": "示例插件",
  "description": "问候和工程面板",
  "author": "Example Author",
  "version": "1.0.0",
  "engines": { "mcdh": ">=1.3.0, <2.0.0" },
  "entry": "main.js",
  "commands": [{
    "id": "hello",
    "title": "问候",
    "menus": ["global", "component"],
    "input_schema": {
      "type": "object",
      "properties": { "name": { "type": "string", "title": "名称", "default": "World" } },
      "required": ["name"],
      "additionalProperties": false
    },
    "output_schema": { "type": "object", "properties": { "message": { "type": "string" } }, "required": ["message"] }
  }],
  "panels": [{ "id": "main", "title": "示例面板", "entry": "panel.js" }],
  "mcp_tools": [{ "name": "hello", "description": "Return a greeting", "command": "hello" }],
  "settings_schema": {
    "type": "object",
    "properties": { "prefix": { "type": "string", "title": "问候前缀", "default": "Hello" } },
    "required": ["prefix"],
    "additionalProperties": false
  }
}
```

- `schema_version`、`api_version` 当前均为 `1`。`engines.mcdh` 使用 Rust `semver::VersionReq` 语法，例如 `>=1.3.0, <2.0.0`；不使用 npm 的 `||` 语法。
- 插件、命令、面板和工具 ID 必须以小写 ASCII 字母开头，后续仅允许小写字母、数字、点、短横线和下划线，最多 64 字节，不能包含 `..` 或 `__`。插件 ID 还须是合法 Windows 路径片段，不能是设备名或以点结尾，`registry` 保留给宿主。MCP 完整名称还须不超过 128 字节。
- `entry`、面板 `entry` 是包内相对路径，使用 `/`；入口必须存在，不能指向源码构建之前的缺失产物。
- `commands`、`panels`、`mcp_tools` 可省略；命令和面板 ID 分别唯一。MCP 声明只能引用同一插件已声明的命令。
- `menus` 可包含 `global` 和 `component`。组件菜单调用会设置 `ctx.environment.componentId`；全局调用和独立 MCP 默认没有当前组件。
- 输入和设置 Schema 必须声明 `type: "object"`。输出 Schema 可描述任意 JSON 值。仅允许 `#` 开头的本地 `$ref`，不能引用网络或本机文件。
- 设置初始值由顶层 `properties` 的 `default` 组成，必须通过整个设置 Schema 校验；必填字段要有有效默认值。嵌套默认值请直接写在顶层对象属性的 `default` 中。输入 Schema 的默认值用于桌面表单，不会在 MCP 调用中自动补参。
- 设置表单支持字符串、数字、布尔和枚举；对象、数组及复杂值使用 JSON 输入。所有设置和命令最终都由 Rust 再次按 Schema 验证。

## 后台生命周期

SDK 类型位于 [sdk/mcdh.d.ts](../sdk/mcdh.d.ts)，可在作者的 TypeScript 工程中作为类型引用，运行时通过传入的 `ctx` 使用，无需导入 npm 包：

```js
// @ts-check
/** @param {import('./mcdh').PluginContext} ctx */
export function activate(ctx) {
  ctx.commands.register("hello", async ({ name }) => {
    const settings = /** @type {{prefix: string}} */ (await ctx.settings.get());
    ctx.log.info("hello", name, ctx.environment.componentId);
    await ctx.storage.set("lastName", name);
    return { message: `${settings.prefix} ${name}` };
  });
  ctx.subscriptions.push(() => { /* release plugin-owned resources */ });
}

export async function deactivate() {
  // Optional, awaited after the command on normal completion or errors.
}
```

每次命令使用独立工作线程及新的 QuickJS 实例，调用 `activate(ctx)`，执行注册的命令，然后调用可选 `deactivate()` 并清理订阅。不是永久驻留后台，不能依赖模块全局变量跨调用保存状态；持久状态使用设置或存储。同一进程最多并发 16 个后台任务，超出时排队，可取消。

后台只提供 ECMAScript 和 SDK，不提供 Node.js、DOM、`fetch`、浏览器定时器或 npm 模块解析。依赖须使用打包后的相对模块；后台模块可以相对导入包内 `.js` 文件。宿主共享上下文模块内部名称是 `mcdh:sdk`，作者通常无需直接导入。

命令入参最多 128 KiB，结果最多 2 MiB；必须可 JSON 序列化，不要返回循环引用、BigInt 或函数。后台内存限制 64 MiB，命令默认超时 60 秒，宿主调用允许设置 1–600,000 毫秒。异常、超时和取消返回 `{code, message}`。死循环会被 QuickJS 中断。超时和强制中断不保证执行 JavaScript 的 `deactivate()`；宿主持有的进程树和异步任务仍会取消、清理。

所有发起的异步 SDK 操作都应 `await`；不要在命令返回后尝试继续运行。`ctx.subscriptions` 接受同步清理函数，按逆序清理，某项失败后仍尝试其余项。

## SDK 接口

以 [TypeScript 类型](../sdk/mcdh.d.ts) 为接口定义。参数使用核心服务现有的 `snake_case` 请求结构，返回 JSON；SDK 方法名使用 `camelCase`。

| 接口 | 行为与边界 |
| --- | --- |
| `environment` | `caller`、`pluginId`、`appVersion`、`packageDir`、`dataDir`、可空 `componentId`；代码目录不要写入 |
| `commands.register(id, handler)` | 仅后台，必须在清单中声明，不得重复注册；返回取消注册函数 |
| `commands.invoke(id, input)` | 仅面板，调用自身后台命令，携带当前组件和版本指纹 |
| `components.list/get` | 使用已有组件发现和读取服务；`list` 与内置刷新一致，首次按应用规则发现 MCS 来源，之后刷新已登记来源，不递归遍历整个磁盘 |
| `components.create/import/copy/move/export` | 复用现有核心工作流、路径检查、组件修改锁和游戏会话保护 |
| `components.setMetadata/regenerateUuids/bumpVersion` | 复用现有元数据、JSONC 定点修改和版本操作；`bumpVersion({component_id, part})` 的 `part` 为 `major/minor/patch` |
| `fs.readText/writeText` | 本机绝对路径，UTF-8，单次最多 2 MiB；写入同目录临时文件后原子替换，父目录须存在 |
| `fs.readDirectory/stat/mkdir/remove` | 列目录最多 10,000 项；`remove` 仅文件或空目录，不递归；这不限制插件通过外部程序执行其他操作 |
| `http.request({url, method, headers, body})` | HTTP/HTTPS，最多 5 次重定向，文本请求/响应最多 2 MiB，响应含 `status/headers/body`；非 2xx 状态作为响应返回 |
| `process.run({program, args, cwd, env, timeout_ms})` | `program` 是本机绝对 EXE 路径，`args` 是字符串数组，不隐式经过 shell；默认工作目录为插件数据目录，`env` 覆盖继承环境，窗口隐藏 |
| `settings.get/set` | SQLite 保存，经清单设置 Schema 校验；`set` 替换整个设置对象 |
| `storage.get/set/delete` | 插件数据目录内 JSON KV 存储；键 1–256 字节，总存储文件最多 2 MiB；缺失键返回 `null` |
| `log.info/warn/error` | 有界本机日志，后台 `console.log/info/warn/error` 也写这里，不写 MCP stdout |
| `ui.openPanel(panelId, placement?, componentId?)` | 声明中的面板，`placement` 为 `main` 或 `window`；后台成功返回后由桌面处理界面动作；MCP 返回 `ui_unavailable` |

Windows 进程使用 Job Object 管理整棵进程树，取消、超时和宿主正常释放都会结束相关进程。进程超时不超过命令剩余时间，stdout/stderr 各保留最多 512 KiB，超出仍持续排空并标记 `truncated`。非零退出码正常返回，由插件决定是否报错。调用 `cmd.exe`、PowerShell 或解释器需要插件显式提供其绝对路径和参数，宿主不自动推断脚本类型。

设置和 KV 存储使用跨进程注册表锁。执行中的插件持有共享租约，安装、更新、卸载、启用状态或授权变更需要独占租约；冲突返回 `plugin_busy`，请先结束或取消桌面 / MCP 中的任务。不能通过缓存绕过状态检查。

## 双模式面板

同一 `mount(container, ctx)` 模块既可挂载主工作区，也可打开独立工具窗口。请只操作传入容器，并返回清理函数：

```js
export function mount(container, ctx) {
  const button = document.createElement("button");
  button.textContent = "问候";
    const run = async () => {
    try {
      const result = await ctx.commands.invoke("hello", { name: "World" });
      button.textContent = result.message;
    } catch (error) {
      button.textContent = error.message;
    }
  };
  button.addEventListener("click", run);
  container.append(button);
  const stopTheme = ctx.onThemeChanged(theme => {
    container.dataset.theme = theme.mode;
  });
  return () => {
    button.removeEventListener("click", run);
    stopTheme();
    button.remove();
  };
}
```

`ctx.theme` 包含 `mode` 和 `accent`，主题变化时更新；`ctx.onThemeChanged` 返回取消监听函数。Shadow DOM 可以继承宿主 CSS 变量，例如 `--text`、`--muted`、`--accent`、`--border`、`--control-bg`。插件自行注入容器内 `<style>`，不得向应用页面注入全局样式。外部事件、观察器和监听请注册清理或放入 `ctx.subscriptions`。

使用 `ctx.assetUrl("logo.png")` 获取包内资源地址，ES module 依赖使用相对导入。资源由 `mcdh-plugin` 自定义协议加载，仅提供当前已启用、信任且校验通过版本清单中的文件；不映射整个磁盘。不得使用远程脚本、CDN、`eval`、`new Function` 或外部源码构建器。CSP 只增加插件本地协议来源，没有增加全局 `unsafe-eval`。

关闭、切换、停用或更新面板后，宿主清理容器和订阅并取消未完成调用。切回组件视图保留原有筛选与排序。主窗口与独立窗口共享插件数据，但各自拥有独立 DOM 生命周期。

## MCP 授权与结果

安装后 MCP 授权默认关闭。只有在桌面插件页单独勾选“允许 MCP 调用”并确认完整权限警告，MCP 才暴露该版本清单声明的工具。MCP 没有安装、更新或授予插件权限的管理工具。

工具命名为 `plugin__<插件ID>__<工具name>`，例如 `plugin__example.hello__hello`。输入按其命令的 `input_schema` 校验。成功结果的 `structuredContent` 是 `{ "result": <命令返回值> }`；声明了 `output_schema` 时，MCP 输出 Schema 也以 `result` 包装。失败设置 `isError: true`，文本内容为 `{code, message}` JSON。

桌面关闭时，独立 `mcdh-mcp.exe` 仍可调用后台插件，不启动 WebView；界面操作返回 `ui_unavailable`。MCP 任务取消会传递到同一运行时。原有 27 个内置工具的名称和行为不变。

桌面与 MCP 读取相同 SQLite 注册表。MCP 每秒检查版本变化并发送 `notifications/tools/list_changed`；每次调用仍重新检查启用状态、版本指纹、完整文件清单和当前授权。更新到不同内容后 MCP 授权撤销；检测到代码变化会撤销信任、停用插件并清除 MCP 授权。修复安装后仍需单独重新授权 MCP。列举工具不执行插件，也不以列表缓存代替调用时的完整性校验。

## 数据位置与诊断

默认应用数据目录为 `%LOCALAPPDATA%\MCDH`，使用 `MCDH_DATA_DIR` 时跟随现有索引位置：

```text
mcdh.db                         SQLite 设置 plugins.registry.v1
plugins/
  installed/<id>/<fingerprint>/ 只读约定的版本代码
  data/<id>/                   独立可写数据
    storage.json
    settings.retained.json     默认卸载时保留的设置和来源
    runtime.log
    runtime.previous.log
  locks/                       跨进程共享 / 独占租约
```

管理页展示最近执行错误，并可查看最新 200 条日志。单条日志限制 8,192 字符；约 1 MiB 后轮换一份旧日志。不要把密码、访问令牌和个人文件内容写入日志。需要联网的插件应在其自己的文档中说明目的和数据用途。

完整性失败请从原仓库重新预览、确认并安装，不要手工编辑版本目录。更新或卸载提示 `plugin_busy` 时，先取消或等待正在使用该插件的任务。GitHub 未登录 API 限流会明确报错，请稍后重试；不降级到不可信镜像。

## 开发验证

宿主开发需要 Rust MSVC、C++ Build Tools、Node.js / pnpm。锁定 `rquickjs = 0.14.0`，测试首先验证 Windows x64 MSVC 的模块、异步宿主调用、死循环中断和内存限制，再运行安装与 SDK 测试。

```powershell
cargo run -p mcdh-plugins --example export_schema -- docs/plugin-manifest.schema.json
pnpm test
pnpm build
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
pnpm test:plugins:ui
```

浏览器测试需要本机 Edge，使用已构建的 `dist` 和隔离的静态测试服务器，检查桌面、窄屏及移动视口的安装预览和面板，并输出截图到 `target/plugin-qa/browser`。这是开发依赖，不是用户运行时依赖。

真实 WebView2 测试先将示例打为根目录正确的本地 ZIP，再使用显式的隔离数据目录安装，最后运行嵌入生产资源的 Tauri 构建：

```powershell
New-Item -ItemType Directory -Force target/plugin-qa | Out-Null
Compress-Archive -Path examples/plugins/hello/* -DestinationPath target/plugin-qa/mcdh-plugin.zip
cargo run -p mcdh-plugins --example install_fixture -- D:/absolute/isolated/plugin-qa-data D:/absolute/mcdh-plugin.zip
pnpm build
cargo build --release -p mcdh-desktop --features tauri/custom-protocol
$env:MCDH_PLUGIN_QA_DATA = 'D:/absolute/isolated/plugin-qa-data'
$env:MCDH_PLUGIN_QA_EXE = (Resolve-Path target/release/mcdh-desktop.exe).Path
pnpm test:plugins:native
```

`install_fixture` 只用于安装开发者自己生成的可信测试包，不向用户界面开放本地绕过安装，不默认授予 MCP 权限。原生脚本启动自己的隐藏测试进程，使用隔离数据和 WebView2 配置，拒绝附加到占用中的调试端口，验证真实命令、图片、模块、主面板、独立窗口与主题同步，然后关闭该测试实例。截图和日志位于 `target/plugin-qa/native`。不要对日常数据目录执行此测试。
