import { chromium, expect } from "@playwright/test";
import { spawn } from "node:child_process";
import { mkdir, writeFile } from "node:fs/promises";
import path from "node:path";

const data = path.resolve(process.env.MCDH_PLUGIN_QA_DATA ?? "target/plugin-qa/native-data");
const output = path.resolve("target/plugin-qa/native");
const endpoint = "http://127.0.0.1:9224";
let occupied = false;
try { await fetch(`${endpoint}/json/version`, {signal:AbortSignal.timeout(500)}); occupied = true; } catch { /* No debugger owns the isolated test port. */ }
if (occupied) throw new Error("Port 9224 is already occupied; refusing to attach to an unrelated WebView.");
await mkdir(output, {recursive:true});
const executable = path.resolve("target/debug/mcdh-desktop.exe");
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
  await main.getByRole("button", {name:/^插件(?:\s*\d+)?$/}).click();
  await expect(main.getByRole("heading", {name:/组件概览/})).toBeVisible();
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
  await panel.screenshot({path:path.join(output,"independent-dark.png")});
  await panel.getByRole("button", {name:"关闭窗口",exact:true}).click();
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
