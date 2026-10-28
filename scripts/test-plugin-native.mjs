import { chromium, expect } from "@playwright/test";
import { spawn } from "node:child_process";
import { mkdir, writeFile } from "node:fs/promises";
import path from "node:path";

const data = path.resolve(process.env.MCDH_PLUGIN_QA_DATA ?? "target/plugin-qa/native-data");
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
  await main.getByRole("button", {name:/^插件(?:\s*\d+)?$/}).click();
  await expect(main.getByRole("heading", {name:/组件概览/})).toBeVisible();
  const mcdeSwitch = main.getByRole("switch", {name:"启用 MCDE"});
  await expect(mcdeSwitch).toBeChecked();
  await mcdeSwitch.click();
  await expect(mcdeSwitch).not.toBeChecked();
  const blocked = await main.evaluate(async () => {
    const invoke = window.__TAURI_INTERNALS__.invoke;
    const mcde = (await invoke("list_plugins")).find(plugin => plugin.manifest.id === "mcdh.mcde");
    const errors = [];
    try { await invoke("launch_component_editor", {componentId:"qa-never-launch"}); } catch (error) { errors.push(error); }
    try { await invoke("execute_plugin_command", {request:{plugin_id:mcde.manifest.id,fingerprint:mcde.fingerprint,command:"open-editor",input:{},component_id:"qa-never-launch"},operationId:crypto.randomUUID()}); } catch (error) { errors.push(error); }
    return errors;
  });
  expect(blocked).toHaveLength(2);
  expect(blocked[0].message).toContain("Plugin is disabled");
  expect(blocked[1].code).toBe("plugin_disabled");
  await mcdeSwitch.click();
  await expect(mcdeSwitch).toBeChecked();
  const repository = main.getByLabel("GitHub 插件仓库");
  await repository.fill("owner/repo");
  await main.getByRole("button", {name:"清空插件仓库"}).click();
  await expect(repository).toHaveValue("");
  await expect(repository).toBeFocused();
  await expect(repository).toHaveCSS("outline-style", "none");
  await expect(main.getByRole("button", {name:"添加插件"})).toHaveCSS("cursor", "not-allowed");
  await main.screenshot({path:path.join(output,"manager.png")});
  await main.getByRole("button", {name:"组件概览",exact:true}).last().click();
  await expect(main.locator(".plugin-panel-mount .stats")).toBeVisible({timeout:20_000});
  await expect(main.locator(".plugin-panel-mount .stat strong").first()).toHaveText("0");
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
  await main.getByRole("button", {name:/^插件(?:\s*\d+)?$/}).click();
  await main.getByLabel("GitHub 插件仓库").fill("owner/repo");
  await main.screenshot({path:path.join(output,"manager-dark.png")});
  expect(errors).toEqual([]);
  await main.getByRole("button", {name:"关闭窗口",exact:true}).click();
  await expect.poll(() => child.exitCode, {timeout:10_000}).not.toBeNull();
  console.log(`Native packaged-resource WebView2 validation passed; screenshots: ${output}`);
} catch (error) {
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
