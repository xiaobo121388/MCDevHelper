import { chromium, expect } from "@playwright/test";
import { mkdir, readFile } from "node:fs/promises";
import path from "node:path";
import { createServer } from "node:http";

const server = createServer(async (request, response) => {
  try {
    const pathname = decodeURIComponent(new URL(request.url, "http://localhost").pathname);
    if (pathname === "/favicon.ico") { response.writeHead(204).end(); return; }
    const example = pathname.startsWith("/examples/plugins/hello/");
    const root = path.resolve(example ? "examples/plugins/hello" : "dist");
    const file = path.resolve(root, example ? pathname.slice("/examples/plugins/hello/".length) : pathname === "/" ? "index.html" : pathname.slice(1));
    if (!file.startsWith(`${root}${path.sep}`)) { response.writeHead(403).end(); return; }
    const bytes = await readFile(file);
    const mime = {".html":"text/html; charset=utf-8",".js":"text/javascript; charset=utf-8",".css":"text/css; charset=utf-8",".png":"image/png"}[path.extname(file)] ?? "application/octet-stream";
    response.writeHead(200, {"Content-Type":mime}).end(bytes);
  } catch { response.writeHead(404).end(); }
});
await new Promise(resolve => server.listen(0,"127.0.0.1",resolve));
const base = process.env.MCDH_UI_URL ?? `http://127.0.0.1:${server.address().port}`;
const output = path.resolve("target/plugin-qa/browser");
await mkdir(output, { recursive: true });
const { version: appVersion } = JSON.parse(await readFile(new URL("../package.json", import.meta.url), "utf8"));
const manifest = JSON.parse(await readFile(new URL("../examples/plugins/hello/mcdh-plugin.json", import.meta.url), "utf8"));
const browser = await chromium.launch({ channel:"msedge", headless:true, args:["--no-proxy-server"] });
const errors = [];
try {
  const viewports = [["desktop",{width:1180,height:760}],["compact",{width:900,height:620}],["mobile",{width:390,height:844}]];
  for (const [name, viewport, theme] of viewports.flatMap(([name,viewport]) => ["light","dark"].map(theme => [`${name}-${theme}`,viewport,theme]))) {
    const context = await browser.newContext({ viewport });
    context.setDefaultTimeout(10_000);
    context.on("page", page => page.on("pageerror", error => errors.push(`${name}: ${error.message}`)));
    const page = await context.newPage();
    let settingsPage;
    await context.exposeBinding("__qaOpenSettings", async () => {
      if (!settingsPage || settingsPage.isClosed()) {
        settingsPage = await context.newPage();
        await settingsPage.goto(`${base}/?dialog=settings`, {waitUntil:"commit"});
      }
      return "dialog-settings";
    });
    await context.exposeBinding("__qaEmitTo", async (_source, args) => {
      expect(args.target.label).toBe("main");
      await page.evaluate(({event,payload}) => window.__qaDispatch(event,payload), args);
    });
    page.on('response', response => { if (response.status() >= 400) console.error('HTTP', response.status(), response.url()); });
    page.setDefaultTimeout(10_000);
    page.on("requestfailed", (request) => console.error("request failed", request.url(), request.failure()));
    page.on("console", (message) => { if (message.type() === "error") console.error("browser", message.text()); });
    page.on("pageerror", (error) => { errors.push(`${name}: ${error.message}`); console.error(error.message); });
    await context.addInitScript(({ manifest, base, theme, appVersion }) => {
      if (location.origin === "null") return;
      const dialog = new URL(location.href).searchParams.get("dialog");
      if (dialog === "settings") window.__MCDH_DIALOG__ = {kind:"settings"};
      const plugin = { manifest, repository:"mcdh-examples/hello",source:{kind:"release",reference:"v1.0.0",archive_sha256:"a".repeat(64)},fingerprint:"b".repeat(64),enabled:true,trusted:true,compatible:true,allow_mcp:false,settings:{label:"组件概览"},last_error:null };
      const tool = {...plugin, manifest:{...manifest,id:"example.tools",name:"示例工具",description:"独立插件操作",commands:[{id:"action",title:"执行示例操作",menus:["component"],input_schema:{type:"object",properties:{}}}],panels:[],mcp_tools:[],settings_schema:{type:"object"}},repository:"example/tools",source:{kind:"release",reference:"v1.0.0",archive_sha256:"a".repeat(64)},settings:{}};
      const components = [
        {id:"one",name:"光影工作室",kind:"addon",path:"D:/Projects/Studio"},
        {id:"two",name:"海岛地图",kind:"map",path:"D:/Projects/Island"},
        {id:"three",name:"像素材质",kind:"material",path:"D:/Projects/Textures"},
      ].map((component) => ({...component,origin:{kind:"single"},manifests:[],tags:[],favorite:false,size_bytes:4096}));
      const callbacks = new Map();
      const listeners = new Map();
      const toolInstalled = () => localStorage.getItem("qa.tool-installed") !== "false";
      const syncTool = () => { tool.enabled = localStorage.getItem("qa.tool-enabled") !== "false"; };
      const mcdkStatus = () => {
        syncTool();
        return {available:false,current_version:null,auto_update:false,phase:"idle",latest_version:null,last_checked_at:null,error:null,downloaded_bytes:0,download_size:null,session:null,last_exit:null};
      };
      window.__qaDispatch = (event,payload) => callbacks.get(listeners.get(event))?.({event,id:0,payload});
      const emitMcdk = () => window.__qaDispatch("mcdk-status-changed",mcdkStatus());
      window.addEventListener("storage", emitMcdk);
      let next = 1;
      localStorage.setItem("mcdh.last-launched-version", appVersion);
      window.isTauri = true;
      window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener() {} };
      window.__TAURI_INTERNALS__ = {
        metadata:{currentWindow:{label:dialog ? "dialog-settings" : "main"},currentWebview:{label:dialog ? "dialog-settings" : "main"}},
        transformCallback(fn) { const id = next++; callbacks.set(id,fn); return id; },
        unregisterCallback(id) { callbacks.delete(id); },
        convertFileSrc(file) { return `${base}/examples/plugins/hello/${file.split('/').slice(2).join('/')}`; },
        async invoke(command, args = {}) {
          if (command === "open_dialog_window" && args.request.kind === "settings") return window.__qaOpenSettings();
          if (command === "plugin:window|get_all_windows") return ["main","dialog-settings"];
          if (command === "plugin:event|emit_to") return window.__qaEmitTo(args);
          if (command === "plugin:event|listen") { listeners.set(args.event,args.handler); return next++; }
          if (command.startsWith("plugin:event|")) return next++;
          if (command === "plugin:window|is_maximized") return false;
          if (command === "get_settings") return {developer_nickname:"MCDH",developer_account:"mcdh@local.invalid",developer_user_id:"0",theme,color_preset:"fluent"};
          if (command === "app_version") return appVersion;
          if (command === "check_for_updates") return {current_version:appVersion,update_available:false,no_release:false};
          if (command === "mcdk_status") return mcdkStatus();
          if (command === "refresh_components") return {components,sources:[],warnings:[]};
          if (command === "list_plugins") { syncTool(); return toolInstalled() ? [tool,plugin] : [plugin]; }
          if (command === "uninstall_plugin" && args.pluginId === "example.tools") { localStorage.setItem("qa.tool-installed","false"); emitMcdk(); return; }
          if (command === "configure_plugin" && args.pluginId === "example.tools") { localStorage.setItem("qa.tool-enabled",String(args.enabled)); emitMcdk(); return tool; }
          if (command === "plugin:dialog|open") return args.options?.directory ? "D:/plugins/source" : "D:/plugins/example.zip";
          if (command === "preview_local_plugin") return {token:"local-preview",...plugin,repository:"Local import",source:{kind:args.path.endsWith(".zip") ? "local_zip" : "local_directory",reference:args.path,archive_sha256:"a".repeat(64)},already_installed:false};
          if (command === "preview_plugin") { args.onProgress.onmessage({phase:"validating",downloaded_bytes:1024,total_bytes:1024}); return {token:"preview",...plugin,already_installed:false}; }
          if (command === "get_plugin_panel") return {plugin,panel:manifest.panels[0],environment:{caller:"desktop",pluginId:manifest.id,appVersion:appVersion,packageDir:"D:/plugins/hello",dataDir:"D:/plugins/data",componentId:args.componentId ?? null}};
          if (command === "execute_plugin_command") return {value:{label:"组件概览",total:3,addon:1,map:1,material:1,current:null,components},actions:[]};
          if (command === "plugin_panel_call") return {value:null,actions:[]};
          if (command === "get_plugin_logs") return [];
          if (command.startsWith("list_")) return [];
          return null;
        },
      };
    }, { manifest, base, theme, appVersion });
    await page.goto(base, {waitUntil:"commit",timeout:10_000});
    if (errors.length) console.error((await page.locator("body").innerText()).slice(0,1000));
    const search = page.getByPlaceholder("搜索名称或路径");
    await search.fill("光影");
    await page.getByRole("button", {name:"清空组件搜索"}).click();
    await expect(search).toHaveValue("");
    await expect(search).toBeFocused();
    await expect(page.getByRole("navigation", {name:"组件分类"}).getByRole("button", {name:/插件/})).toHaveCount(0);
    await page.getByRole("button", {name:"设置",exact:true}).click();
    await expect.poll(() => !!settingsPage).toBe(true);
    const manager = settingsPage;
    await manager.getByRole("navigation", {name:"设置分类"}).getByRole("button", {name:"插件",exact:true}).click();
    await expect(manager.getByRole("heading", {name:"插件",exact:true})).toBeVisible();
    await expect(manager.getByRole("heading", {name:/组件概览/})).toBeVisible();
    await expect(manager.getByRole("button", {name:"保存设置"})).toHaveCount(0);
    const toolSwitch = manager.getByRole("switch", {name:"启用 示例工具"});
    await expect(toolSwitch).toBeChecked();
    await expect(toolSwitch).toHaveCSS("appearance", "none");
    await expect(toolSwitch).toHaveCSS("border-radius", "999px");
    await expect(toolSwitch).toHaveCSS("width", "36px");
    await expect(toolSwitch).toHaveCSS("height", "20px");
    await expect(toolSwitch.locator("..")).toHaveText("");
    await toolSwitch.focus();
    await manager.keyboard.press("Space");
    await expect(toolSwitch).not.toBeChecked();
    await expect(toolSwitch).toBeFocused();
    await manager.keyboard.press("Space");
    await expect(toolSwitch).toBeChecked();
    await toolSwitch.click();
    await expect(toolSwitch).not.toBeChecked();
    await toolSwitch.click();
    await expect(toolSwitch).toBeChecked();
    const switchBounds = await toolSwitch.boundingBox();
    const titleBounds = await manager.getByRole("heading", {name:/^示例工具/}).boundingBox();
    expect(Math.abs(switchBounds.y - titleBounds.y)).toBeLessThan(6);
    for (const row of await manager.locator(".plugin-row").all()) {
      const uninstall = row.locator(".plugin-uninstall");
      await expect(uninstall).toHaveText("");
      await expect(uninstall).toHaveAttribute("title", /^卸载 /);
      for (const button of await row.locator(".plugin-row-actions button").all()) {
        await expect(button).toHaveCSS("height", "38px");
      }
      for (const button of await row.locator(".plugin-row-actions .icon-button").all()) {
        await expect(button).toHaveCSS("width", "38px");
      }
    }
    await manager.locator(".plugin-row").filter({has:manager.getByRole("heading", {name:/^组件概览/})}).screenshot({path:path.join(output,`${name}-plugin-row.png`)});
    await manager.getByRole("button", {name:"卸载 示例工具"}).click();
    await expect(manager.getByRole("checkbox", {name:"同时清除插件设置、存储和日志"})).not.toBeChecked();
    await manager.getByRole("button", {name:"确认卸载"}).click();
    await expect(toolSwitch).toHaveCount(0);
    await manager.screenshot({path:path.join(output,`${name}-uninstalled.png`)});
    await page.getByRole("button", {name:/^全部组件/}).click();
    await page.getByRole("button", {name:"组件插件操作"}).first().click();
    await expect(page.getByRole("menuitem", {name:"执行示例操作"})).toHaveCount(0);
    await page.keyboard.press("Escape");
    const repository = manager.getByLabel("GitHub 插件仓库");
    await expect(manager.getByRole("button", {name:"添加插件"})).toHaveCSS("cursor", "not-allowed");
    await repository.fill("owner/repo");
    await manager.getByRole("button", {name:"清空插件仓库"}).click();
    await expect(repository).toHaveValue("");
    await expect(repository).toBeFocused();
    await expect(repository).toHaveCSS("outline-style", "none");
    await expect(repository.locator("..")).not.toHaveCSS("box-shadow", "none");
    await manager.screenshot({path:path.join(output,`${name}-manager.png`)});
    expect(await manager.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
    expect(await manager.locator(".settings-panel-body").evaluate(node => node.scrollWidth <= node.clientWidth)).toBe(true);
    await repository.fill("mcdh-examples/hello");
    await manager.getByRole("button", {name:"添加插件"}).click();
    await expect(manager.getByRole("button", {name:"确认安装并启用"})).toBeDisabled();
    await manager.screenshot({path:path.join(output,`${name}-preview.png`)});
    await manager.getByRole("button", {name:"关闭安装预览"}).click();
    await manager.getByRole("button", {name:"本地导入"}).click();
    await expect(manager.getByText("本地包 · D:/plugins/example.zip")).toBeVisible();
    await expect(manager.getByRole("button", {name:"确认安装并启用"})).toBeDisabled();
    await manager.getByRole("button", {name:"关闭安装预览"}).click();
    await manager.getByRole("button", {name:"导入插件目录"}).click();
    await expect(manager.getByText("本地包 · D:/plugins/source")).toBeVisible();
    await manager.getByRole("button", {name:"关闭安装预览"}).click();
    await manager.getByRole("button", {name:"组件概览",exact:true}).click();
    await expect(page.locator(".plugin-panel-mount .stats")).toBeVisible({timeout:20_000});
    await expect(page.locator(".plugin-panel-mount .stat strong").first()).toHaveText("3");
    const image = page.locator(".plugin-panel-mount img");
    await expect(image).toBeVisible();
    await expect.poll(() => image.evaluate((image) => image.naturalWidth)).toBeGreaterThan(0);
    await page.getByRole("button", {name:"读取组件",exact:true}).click();
    await page.screenshot({path:path.join(output,`${name}-panel.png`)});
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
    await context.close();
  }
  expect(errors).toEqual([]);
  console.log(`Plugin UI verified in light/dark themes at 1180, 900 and 390px; screenshots: ${output}`);
} finally { await browser.close(); await new Promise(resolve => server.close(resolve)); }
