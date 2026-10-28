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
const manifest = JSON.parse(await readFile(new URL("../examples/plugins/hello/mcdh-plugin.json", import.meta.url), "utf8"));
const browser = await chromium.launch({ channel:"msedge", headless:true, args:["--no-proxy-server"] });
const errors = [];
try {
  for (const [name, viewport] of [["desktop",{width:1180,height:760}],["compact",{width:900,height:620}],["mobile",{width:390,height:844}]]) {
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
    await context.addInitScript(({ manifest, base }) => {
      if (location.origin === "null") return;
      const dialog = new URL(location.href).searchParams.get("dialog");
      if (dialog === "settings") window.__MCDH_DIALOG__ = {kind:"settings"};
      const plugin = { manifest, repository:"mcdh-examples/hello",source:{kind:"release",reference:"v1.0.0",archive_sha256:"a".repeat(64)},fingerprint:"b".repeat(64),enabled:true,trusted:true,compatible:true,allow_mcp:false,settings:{label:"组件概览"},last_error:null };
      const mcde = {...plugin, manifest:{...manifest,id:"mcdh.mcde",name:"MCDE",description:"MCDevEditor 原生编辑器",commands:[{id:"open-editor",title:"在 MCDE 中打开",menus:["component"],input_schema:{type:"object",properties:{}}}],panels:[],mcp_tools:[],settings_schema:{type:"object"}},repository:"MCDH",source:{kind:"builtin",reference:"MCDH 1.3.0",archive_sha256:""},settings:{}};
      const components = [
        {id:"one",name:"光影工作室",kind:"addon",path:"D:/Projects/Studio"},
        {id:"two",name:"海岛地图",kind:"map",path:"D:/Projects/Island"},
        {id:"three",name:"像素材质",kind:"material",path:"D:/Projects/Textures"},
      ].map((component) => ({...component,origin:{kind:"single"},manifests:[],tags:[],favorite:false,size_bytes:4096}));
      const callbacks = new Map();
      const listeners = new Map();
      const mcdeInstalled = () => localStorage.getItem("qa.mcde-installed") !== "false";
      const syncMcde = () => { mcde.enabled = localStorage.getItem("qa.mcde-enabled") !== "false"; };
      const mcdkStatus = () => {
        syncMcde();
        return {available:false,current_version:null,auto_update:false,phase:"idle",latest_version:null,last_checked_at:null,error:null,downloaded_bytes:0,download_size:null,session:null,last_exit:null,mcde_plugin_installed:mcdeInstalled(),mcde_plugin_enabled:mcdeInstalled() && mcde.enabled,mcde:mcdeInstalled() && mcde.enabled ? {available:false,configured:false,host_available:false,host_revision:null,plugin_version:null,error:null,directory:null} : null};
      };
      window.__qaDispatch = (event,payload) => callbacks.get(listeners.get(event))?.({event,id:0,payload});
      const emitMcdk = () => window.__qaDispatch("mcdk-status-changed",mcdkStatus());
      window.addEventListener("storage", emitMcdk);
      let next = 1;
      localStorage.setItem("mcdh.last-launched-version", "1.3.0");
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
          if (command === "get_settings") return {developer_nickname:"MCDH",developer_account:"mcdh@local.invalid",developer_user_id:"0",theme:"light",color_preset:"fluent"};
          if (command === "app_version") return "1.3.0";
          if (command === "check_for_updates") return {current_version:"1.3.0",update_available:false,no_release:false};
          if (command === "mcdk_status") return mcdkStatus();
          if (command === "refresh_components") return {components,sources:[],warnings:[]};
          if (command === "list_plugins") { syncMcde(); return mcdeInstalled() ? [mcde,plugin] : [plugin]; }
          if (command === "list_builtin_plugins") return [mcde.manifest];
          if (command === "install_builtin_plugin") { localStorage.setItem("qa.mcde-installed","true"); localStorage.setItem("qa.mcde-enabled","true"); emitMcdk(); return mcde; }
          if (command === "uninstall_plugin" && args.pluginId === "mcdh.mcde") { localStorage.setItem("qa.mcde-installed","false"); emitMcdk(); return; }
          if (command === "configure_plugin" && args.pluginId === "mcdh.mcde") { localStorage.setItem("qa.mcde-enabled",String(args.enabled)); emitMcdk(); return mcde; }
          if (command === "plugin:dialog|open") return args.options?.directory ? "D:/plugins/source" : "D:/plugins/example.zip";
          if (command === "preview_local_plugin") return {token:"local-preview",...plugin,repository:"Local import",source:{kind:args.path.endsWith(".zip") ? "local_zip" : "local_directory",reference:args.path,archive_sha256:"a".repeat(64)},already_installed:false};
          if (command === "preview_plugin") { args.onProgress.onmessage({phase:"validating",downloaded_bytes:1024,total_bytes:1024}); return {token:"preview",...plugin,already_installed:false}; }
          if (command === "get_plugin_panel") return {plugin,panel:manifest.panels[0],environment:{caller:"desktop",pluginId:manifest.id,appVersion:"1.3.0",packageDir:"D:/plugins/hello",dataDir:"D:/plugins/data",componentId:args.componentId ?? null}};
          if (command === "execute_plugin_command") return {value:{label:"组件概览",total:3,addon:1,map:1,material:1,current:null,components},actions:[]};
          if (command === "plugin_panel_call") return {value:null,actions:[]};
          if (command === "get_plugin_logs") return [];
          if (command.startsWith("list_")) return [];
          return null;
        },
      };
    }, { manifest, base });
    await page.goto(base, {waitUntil:"commit",timeout:10_000});
    if (errors.length) console.error((await page.locator("body").innerText()).slice(0,1000));
    const search = page.getByPlaceholder("搜索名称或路径");
    await search.fill("光影");
    await page.getByRole("button", {name:"清空组件搜索"}).click();
    await expect(search).toHaveValue("");
    await expect(search).toBeFocused();
    await expect(page.getByRole("button", {name:/^在 MCDE 中打开 /})).toHaveCount(3);
    await expect(page.getByRole("navigation", {name:"组件分类"}).getByRole("button", {name:/插件/})).toHaveCount(0);
    await page.getByRole("button", {name:"设置",exact:true}).click();
    await expect.poll(() => !!settingsPage).toBe(true);
    const manager = settingsPage;
    await manager.getByRole("navigation", {name:"设置分类"}).getByRole("button", {name:"插件",exact:true}).click();
    await expect(manager.getByRole("heading", {name:"插件",exact:true})).toBeVisible();
    await expect(manager.getByRole("heading", {name:/组件概览/})).toBeVisible();
    await expect(manager.getByRole("button", {name:"保存设置"})).toHaveCount(0);
    const mcdeSwitch = manager.getByRole("switch", {name:"启用 MCDE"});
    await expect(mcdeSwitch).toBeChecked();
    await mcdeSwitch.click();
    await expect(mcdeSwitch).not.toBeChecked();
    await expect(manager.getByRole("textbox", {name:"MCDE 插件目录"})).toHaveCount(0);
    await mcdeSwitch.click();
    await expect(mcdeSwitch).toBeChecked();
    await manager.getByRole("button", {name:"卸载 MCDE"}).click();
    await expect(manager.getByRole("checkbox", {name:"同时清除插件管理数据和日志"})).not.toBeChecked();
    await manager.getByRole("button", {name:"确认卸载"}).click();
    await expect(mcdeSwitch).toHaveCount(0);
    await expect(manager.locator("body")).not.toContainText("MCDE");
    await manager.screenshot({path:path.join(output,`${name}-uninstalled.png`)});
    await page.getByRole("button", {name:/^全部组件/}).click();
    await expect(page.getByRole("button", {name:/^在 MCDE 中打开 /})).toHaveCount(0);
    await page.getByRole("button", {name:"组件插件操作"}).first().click();
    await expect(page.getByRole("menuitem", {name:/MCDE/})).toHaveCount(0);
    await page.keyboard.press("Escape");
    await manager.getByRole("button", {name:"内置插件",exact:true}).click();
    await manager.getByRole("button", {name:"安装 MCDE"}).click();
    await expect(mcdeSwitch).toBeChecked();
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
  console.log(`Plugin UI verified at 1180, 900 and 390px; screenshots: ${output}`);
} finally { await browser.close(); await new Promise(resolve => server.close(resolve)); }
