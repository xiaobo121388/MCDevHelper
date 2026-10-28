import { chromium, expect } from "@playwright/test";
import { spawn } from "node:child_process";
import { mkdir, writeFile } from "node:fs/promises";
import path from "node:path";

const data = path.resolve(process.env.MCDH_PLUGIN_QA_DATA ?? "target/plugin-qa/native-plugin-api-data");
if (!data.startsWith(`${path.resolve("target/plugin-qa")}${path.sep}`)) throw new Error("Native plugin QA must use an isolated directory under target/plugin-qa.");
const output = path.resolve("target/plugin-qa/native");
const endpoint = "http://127.0.0.1:9224";
let occupied = false;
try { await fetch(`${endpoint}/json/version`, {signal:AbortSignal.timeout(500)}); occupied = true; } catch { /* No debugger owns the isolated test port. */ }
if (occupied) throw new Error("Port 9224 is already occupied; refusing to attach to an unrelated WebView.");
await mkdir(output, {recursive:true});
const executable = path.resolve(process.env.MCDH_PLUGIN_QA_EXE ?? "target/debug/mcdh-desktop.exe");
const child = spawn(executable, [], { windowsHide:true, env:{...process.env,
  MCDH_DATA_DIR:data, MCDH_DISABLE_MCS_SCAN:"1", WEBVIEW2_USER_DATA_FOLDER:path.join(data,"webview"),
  WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS:"--remote-debugging-port=9224 --remote-debugging-address=127.0.0.1",
}, stdio:["ignore","pipe","pipe"] });
const outputLog = [];
child.stdout.on("data", data => outputLog.push(data.toString()));
child.stderr.on("data", data => outputLog.push(data.toString()));
let browser;
let main;
let settings;
try {
  await expect.poll(async () => {
    if (child.exitCode !== null) throw new Error(`Native application exited: ${outputLog.join('')}`);
    try { return (await fetch(`${endpoint}/json/version`)).status; } catch { return 0; }
  }, {timeout:30_000}).toBe(200);
  browser = await chromium.connectOverCDP(endpoint);
  const context = browser.contexts()[0];
  await expect.poll(async () => {
    for (const page of context.pages()) {
      try {
        if (await page.evaluate(() => window.__TAURI_INTERNALS__?.metadata?.currentWindow?.label) === "main") { main = page; return true; }
      } catch { /* WebView is still initializing. */ }
    }
    return false;
  }, {timeout:30_000}).toBe(true);
  const errors = [];
  context.on("page", page => page.on("pageerror", error => errors.push(error.message)));
  main.on("pageerror", error => errors.push(error.message));
  await expect(main.getByRole("heading", {name:"全部组件",exact:true})).toBeVisible({timeout:30_000});
  for (let attempt = 0; attempt < 5; attempt++) {
    for (const page of context.pages()) {
      if (page === main) continue;
      const label = await page.evaluate(() => window.__TAURI_INTERNALS__?.metadata?.currentWindow?.label).catch(() => "");
      if (label === "dialog-startup") {
        const close = page.getByRole("button", {name:/^(知道了|稍后提醒)$/});
        if (await close.count()) await close.click();
      }
    }
    await main.waitForTimeout(200);
  }
  await main.evaluate(async () => {
    const settings = await window.__TAURI_INTERNALS__.invoke("get_settings");
    await window.__TAURI_INTERNALS__.invoke("set_settings", {settings:{...settings,theme:"light"}});
    await window.__TAURI_INTERNALS__.invoke("plugin:event|emit", {event:"mcdh:settings-changed",payload:null});
  });
  await expect(main.locator("html")).toHaveAttribute("data-theme", "light");
  const initial = await main.evaluate(async () => ({plugins:await window.__TAURI_INTERNALS__.invoke("list_plugins"),status:await window.__TAURI_INTERNALS__.invoke("mcdk_status")}));
  expect(initial.plugins).toEqual([]);
  const imported = await main.evaluate(async (directory) => {
    const invoke = window.__TAURI_INTERNALS__.invoke;
    const previous = (await invoke("list_plugins")).find(plugin => plugin.manifest.id === "example.component-overview");
    if (previous && previous.source.kind !== "local_directory") await invoke("uninstall_plugin", {pluginId:previous.manifest.id,clearData:false});
    const preview = await invoke("preview_local_plugin", {path:directory,operationId:crypto.randomUUID()});
    if (preview.manifest.id !== "example.component-overview") throw new Error("Unexpected QA fixture identity");
    const installed = await invoke("install_plugin", {token:preview.token,fingerprint:preview.fingerprint});
    return installed.source.kind;
  }, path.resolve("examples/plugins/hello"));
  expect(imported).toBe("local_directory");
  await expect(main.getByRole("navigation", {name:"组件分类"}).getByRole("button", {name:/插件/})).toHaveCount(0);
  await main.getByRole("button", {name:"设置",exact:true}).click();
  await expect.poll(async () => {
    for (const page of context.pages()) {
      const label = await page.evaluate(() => window.__TAURI_INTERNALS__?.metadata?.currentWindow?.label).catch(() => "");
      if (label === "dialog-settings") { settings = page; return true; }
    }
    return false;
  }, {timeout:20_000}).toBe(true);
  await settings.getByRole("navigation", {name:"设置分类"}).getByRole("button", {name:"插件",exact:true}).click();
  await expect(settings.getByRole("heading", {name:/组件概览/})).toBeVisible();
  await expect(settings.getByRole("button", {name:"保存设置"})).toHaveCount(0);
  const enabled = settings.getByRole("switch", {name:"启用 组件概览"});
  await expect(enabled).toBeChecked();
  await expect(enabled).toHaveCSS("border-radius", "999px");
  await enabled.click();
  await expect(enabled).not.toBeChecked();
  await expect(settings.getByRole("button", {name:"组件概览",exact:true})).toBeDisabled();
  const blocked = await main.evaluate(async () => {
    const invoke = window.__TAURI_INTERNALS__.invoke;
    const plugin = (await invoke("list_plugins")).find(item => item.manifest.id === "example.component-overview");
    try { await invoke("execute_plugin_command", {request:{plugin_id:plugin.manifest.id,fingerprint:plugin.fingerprint,command:"inspect",input:{}},operationId:crypto.randomUUID()}); }
    catch (error) { return error.code; }
    return null;
  });
  expect(blocked).toBe("plugin_disabled");
  await enabled.click();
  await expect(enabled).toBeChecked();
  const repository = settings.getByLabel("GitHub 插件仓库");
  await repository.fill("owner/repo");
  await settings.getByRole("button", {name:"清空插件仓库"}).click();
  await expect(repository).toHaveValue("");
  await expect(repository).toBeFocused();
  await expect(repository).toHaveCSS("outline-style", "none");
  await expect(settings.getByRole("button", {name:"添加插件"})).toHaveCSS("cursor", "not-allowed");
  expect(await settings.locator(".settings-panel-body").evaluate(node => node.scrollWidth <= node.clientWidth)).toBe(true);
  await settings.screenshot({path:path.join(output,"manager.png")});
  await settings.getByRole("button", {name:"组件概览",exact:true}).click();
  await expect(main.locator(".plugin-panel-mount .stats")).toBeVisible({timeout:20_000});
  await expect(main.locator(".plugin-panel-mount .stat strong").first()).toHaveText("0");
  await expect.poll(() => settings.evaluate(() => window.__TAURI_INTERNALS__.invoke("plugin:window|is_minimized", {label:"dialog-settings"}))).toBe(true);
  const image = main.locator(".plugin-panel-mount img");
  await expect.poll(() => image.evaluate(image => image.naturalWidth)).toBeGreaterThan(0);
  await main.getByRole("button", {name:"读取组件",exact:true}).click();
  await expect(main.locator(".plugin-panel-mount .error")).toBeEmpty();
  await main.screenshot({path:path.join(output,"main-panel.png")});
  await main.getByRole("button", {name:"在独立窗口打开"}).click();
  let panel;
  await expect.poll(async () => {
    for (const page of context.pages()) {
      if (page === main) continue;
      const label = await page.evaluate(() => window.__TAURI_INTERNALS__?.metadata?.currentWindow?.label).catch(() => "");
      if (label?.startsWith("dialog-plugin-")) { panel = page; return true; }
    }
    return false;
  }, {timeout:20_000}).toBe(true);
  await expect(panel.locator(".plugin-panel-mount .stats")).toBeVisible({timeout:20_000});
  await expect.poll(() => panel.locator(".plugin-panel-mount img").evaluate(image => image.naturalWidth)).toBeGreaterThan(0);
  await main.evaluate(async () => {
    const settings = await window.__TAURI_INTERNALS__.invoke("get_settings");
    await window.__TAURI_INTERNALS__.invoke("set_settings", {settings:{...settings,theme:"dark"}});
    await window.__TAURI_INTERNALS__.invoke("plugin:event|emit", {event:"mcdh:settings-changed",payload:null});
  });
  await expect(panel.locator("html")).toHaveAttribute("data-theme", "dark");
  await expect(panel.locator(".plugin-panel-mount .overview")).toHaveAttribute("data-theme", "dark");
  await panel.screenshot({path:path.join(output,"independent-dark.png")});
  await panel.getByRole("button", {name:"关闭窗口",exact:true}).click();
  await main.getByRole("button", {name:"设置",exact:true}).click();
  await expect.poll(() => settings.evaluate(() => window.__TAURI_INTERNALS__.invoke("plugin:window|is_minimized", {label:"dialog-settings"}))).toBe(false);
  await expect(settings.getByRole("heading", {name:"插件",exact:true})).toBeVisible();
  await settings.getByLabel("GitHub 插件仓库").fill("owner/repo");
  await settings.screenshot({path:path.join(output,"manager-dark.png")});
  await settings.getByRole("button", {name:"卸载 组件概览"}).click();
  await expect(settings.getByRole("checkbox", {name:"同时清除插件设置、存储和日志"})).not.toBeChecked();
  await settings.getByRole("button", {name:"确认卸载"}).click();
  await expect(enabled).toHaveCount(0);
  await settings.screenshot({path:path.join(output,"manager-uninstalled.png")});
  const removed = await main.evaluate(() => window.__TAURI_INTERNALS__.invoke("list_plugins"));
  expect(removed).toEqual([]);
  await settings.getByRole("button", {name:"开发工具",exact:true}).click();
  await settings.screenshot({path:path.join(output,"settings-uninstalled.png")});
  await settings.getByRole("button", {name:"关闭窗口",exact:true}).click();
  expect(errors).toEqual([]);
  await main.getByRole("button", {name:"关闭窗口",exact:true}).click();
  await expect.poll(() => child.exitCode, {timeout:10_000}).not.toBeNull();
  console.log(`Native packaged-resource WebView2 validation passed; screenshots: ${output}`);
} catch (error) {
  if (settings && !settings.isClosed()) {
    console.error(await settings.locator("body").innerText().catch(() => "Cannot read settings document"));
    await settings.screenshot({path:path.join(output,"failure-settings.png")}).catch(() => {});
  }
  if (main && !main.isClosed()) {
    console.error(await main.locator("body").innerText().catch(() => "Cannot read native document"));
    await main.screenshot({path:path.join(output,"failure.png")}).catch(() => {});
  }
  throw error;
} finally {
  if (child.exitCode === null && main && !main.isClosed()) {
    await main.evaluate(() => window.__TAURI_INTERNALS__.invoke("plugin:window|close", {label:"main"})).catch(() => {});
    await new Promise(resolve => setTimeout(resolve, 1000));
  }
  if (browser) await browser.close().catch(() => {});
  if (child.exitCode === null) child.kill();
  await writeFile(path.join(output,"host.log"), outputLog.join(""));
}
