const RELEASE_NOTES: Record<string, string[]> = {
  "1.1.0": [
    "新增 .mcdh.json 可携带组件元数据，支持显示名称、标签和收藏状态。",
    "新增收藏视图、卡片快速收藏，以及清洁/完整两种导入导出模式。",
    "导出会记住上次目录，并可在文件重名时选择覆盖或自动添加后缀。",
    "打开组件目录和 VS Code 改为索引直达，避免点击时全量扫描造成卡顿。",
    "启动时自动检查 GitHub Release，并在升级后的首次启动展示更新日志。",
  ],
};

export function releaseNotesFor(version: string): string[] {
  const normalized = version.trim().replace(/^[vV]/, "");
  return RELEASE_NOTES[normalized] ?? ["此版本包含功能改进与问题修复。"];
}
