# MCDK 可视化配置移植

## 来源

- VS Code 扩展：Dofes/mcdev-tools，https://github.com/Dofes/mcdev-tools
- 配置界面参考提交：a31d4a4514593c78745dded5cad19eab0d87fb4b。
- 皮肤功能：xiaobo121388 的 PR #10，https://github.com/Dofes/mcdev-tools/pull/10
- PR 固定提交：733d0f3db347cad671ec0a68a130c05a9c1d6d6b。
- 检查日期：2026-11-10。PR #10 当时为 open、未合并；PR #9 为已关闭的前序版本。本次选择性移植源码，不声称 PR 已合并，不替换 MCDH 的内置 MCDK 二进制。
- 上游采用 MIT 许可，完整声明保存在 docs/licenses/mcdev-tools-MIT.txt，并纳入生成的 THIRD_PARTY_LICENSES.md 与发行包。

## 入口与范围

组件卡片的 MCDK 可视化配置按钮，或组件配置页的“可视化编辑”，打开按组件区分的独立窗口。

六类配置：启动、世界、Mod 目录、皮肤、窗口、调试。可编辑游戏路径、玩家名、日志协议、启动选项、五类热重载、世界名称/种子/类型/模式、游戏规则、实验选项、Mod 启用与热重载、窗口颜色/位置/尺寸/透明度、重载按键和 MCP 服务。保存并启动复用 MCDH 的已安装 MCDK 启动和会话管理。

皮肤功能包括：

- 从所选游戏的 data/skin_packs/vanilla/skins.json 读取 free 条目，仅接收 classic/slim 两种受支持几何模型。
- 自动识别 Slim，预览本地 PNG，不捆绑或下载 Minecraft 皮肤资源。
- 读取上游 PR 的四个已知网易 4D 支持包及其固定预览条目；游戏目录向上三级的 componentcache/support 为原 PR 的定位规则。
- 组件级自定义 PNG 库存入本机 MCDH 数据库，路径归一化去重，最多 64 项。移除只取消库引用，不删除 PNG，不自动清空当前皮肤。
- 识别同一游戏安装目录下旧版本的原版皮肤路径，由用户显式切换至当前版本，不在后台改写配置。
- 异步预览与目录加载忽略过期响应；资源错误在界面显示。

不包含 VS Code 专用的 Python 调试适配器、性能分析面板、运行时 JSON UI 调试器、扩展原生 DLL 发现接口。PR 中额外的 Mod UUID/版本 Diff 审批流程不在本次皮肤移植范围；MCDH 已有 UUID/版本功能保持不变。皮肤预览沿用 PR 的 PNG/4D 固定图，非 3D 模型编辑器。

## 保存和资源边界

首次打开不存在的 .mcdev.json 不写入磁盘；只有明确保存才初始化默认配置。已有配置按允许字段增量修改，保留未知字段、skin_info 附加字段、未修改区域注释与 BOM，采用现有 JSONC CST 和同目录原子写入。更改整个 Mod 列表时，列表值会重建，列表内部注释不保证保留。

读取返回 SHA-256；保存前校验版本并复查内容，拒绝覆盖已检测到的外部修改。MCDH 写操作共用跨进程修改锁，受管理的同组件游戏运行期间禁止保存。任意外部编辑器不遵守该锁，最后内容检查和文件替换之间仍有操作系统级极短竞争窗口；不宣称提供跨所有编辑器的文件事务隔离。不能读取 VS Code 尚未落盘的缓冲区。

配置上限 1 MiB；PNG 上限 8 MiB、2048×2048、解码上限 32 MiB；皮肤目录最多 256 项、读取预算 16 MiB；4D ZIP 上限 2 MiB、1024 个条目，只读固定 PNG，限制解压大小并拒绝重复预览条目。原版目录验证真实路径包含关系，不允许 ../ 或链接越界。

## 验证

- pnpm test
- pnpm build
- cargo test -p mcdh-core
- cargo check -p mcdh-desktop
- node scripts/test-mcdk-editor.mjs：浏览器多视口、明暗主题、所有分类及皮肤预览。
- cargo build -p mcdh-desktop --features tauri/custom-protocol
- node scripts/test-mcdk-editor.mjs --native：隔离索引和组件中运行真实 WebView2，验证 Tauri 命令、保存、冲突与皮肤预览。

测试使用 target/mcdk-editor-qa 下的隔离数据，不能将测试结果等同于真实游戏中的皮肤渲染、4D 动画或所有游戏版本兼容性。不会自动启动用户的游戏或修改真实组件。

### 2026-11-10 验证记录

| 项目 | 结果 |
| --- | --- |
| 前端全量单测 | 178 通过 |
| mcdh-core / mcdh-desktop 单测 | 109 + 20 通过；3 项既有环境依赖测试跳过 |
| TypeScript、Vite、嵌入生产资源的桌面开发构建 | 通过 |
| Chromium / Edge，1000、700、390 像素宽，明暗主题，六个分类 | 36 张截图；无横向溢出；预览像素非空 |
| 原生 WebView2 | 卡片入口、实际配置保存、BOM/注释保留、皮肤库持久化、外部修改拒绝、取消关闭、确认关闭通过 |
| PNG / ZIP 测试资源 | 使用隔离合成目录及 MCDH 自有图标作为 PNG 测试夹具，不代表真实 Minecraft 皮肤效果 |
| 真实游戏中的换肤、4D 模型/动画 | 未验证 |
