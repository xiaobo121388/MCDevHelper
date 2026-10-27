export function mount(container, ctx) {
  let active = true;
  const style = document.createElement("style");
  style.textContent = `
    .overview{padding:24px 28px;letter-spacing:0}.heading{display:flex;align-items:center;gap:12px;margin-bottom:26px}.heading img{width:32px;height:32px}.heading h2{font-size:18px;margin:0;flex:1;overflow-wrap:anywhere}
    button{border:1px solid var(--control-border);background:var(--control-bg);color:var(--text);padding:8px 12px;border-radius:5px;cursor:pointer}button:disabled{opacity:.5;cursor:default}
    .stats{display:grid;grid-template-columns:repeat(4,minmax(0,1fr));border-block:1px solid var(--border);padding:22px 0;gap:16px}.stat span{display:block;color:var(--muted);font-size:12px;margin-bottom:9px}.stat strong{font-size:27px;font-weight:600}
    .current{padding:18px 0;color:var(--muted);font-size:12px;overflow-wrap:anywhere}.rows{display:grid}.row{display:grid;grid-template-columns:minmax(0,1fr) 56px;gap:14px;border-bottom:1px solid var(--border);padding:14px 0}.row strong{font-size:13px;font-weight:500}.row small{display:block;color:var(--subtle);font-size:11px;overflow-wrap:anywhere;margin-top:5px}.kind{font-size:11px;color:var(--accent)}.error{color:var(--danger);font-size:12px;white-space:pre-wrap}
    @media(max-width:500px){.overview{padding:18px}.stats{grid-template-columns:repeat(2,minmax(0,1fr))}.heading{flex-wrap:wrap}}
  `;
  const root = document.createElement("section"); root.className = "overview";
  const heading = document.createElement("header"); heading.className = "heading";
  const image = document.createElement("img"); image.src = ctx.assetUrl("logo.png"); image.alt = "";
  const title = document.createElement("h2"); title.textContent = "组件概览";
  const refresh = document.createElement("button"); refresh.textContent = "读取组件";
  heading.append(image, title, refresh);
  const stats = document.createElement("div"); stats.className = "stats";
  const counts = {};
  for (const [key, label] of [["total", "全部组件"], ["addon", "模组"], ["map", "地图"], ["material", "材质"]]) {
    const cell = document.createElement("div"); cell.className = "stat";
    const name = document.createElement("span"); name.textContent = label;
    const count = document.createElement("strong"); count.textContent = "0"; counts[key] = count;
    cell.append(name, count); stats.append(cell);
  }
  const current = document.createElement("div"); current.className = "current";
  const rows = document.createElement("div"); rows.className = "rows";
  const error = document.createElement("p"); error.className = "error"; error.setAttribute("role", "alert");
  root.append(heading, stats, current, rows, error); container.append(style, root);
  const load = async () => {
    refresh.disabled = true; error.textContent = "";
    try {
      const data = await ctx.commands.invoke("inspect");
      if (!active) return;
      title.textContent = data.label;
      for (const key of Object.keys(counts)) counts[key].textContent = String(data[key]);
      current.textContent = data.current ? `当前组件：${data.current.name} · ${data.current.path}` : "全部组件";
      rows.replaceChildren();
      for (const component of data.components) {
        const row = document.createElement("div"); row.className = "row";
        const content = document.createElement("div");
        const name = document.createElement("strong"); name.textContent = component.name;
        const path = document.createElement("small"); path.textContent = component.path;
        const kind = document.createElement("span"); kind.className = "kind";
        kind.textContent = { addon:"模组", map:"地图", material:"材质" }[component.kind];
        content.append(name, path); row.append(content, kind); rows.append(row);
      }
    } catch (cause) { if (active) error.textContent = cause.message; }
    finally { if (active) refresh.disabled = false; }
  };
  refresh.addEventListener("click", load);
  const stopTheme = ctx.onThemeChanged((theme) => { root.dataset.theme = theme.mode; });
  void load();
  return () => { active = false; refresh.removeEventListener("click", load); stopTheme(); root.remove(); style.remove(); };
}
