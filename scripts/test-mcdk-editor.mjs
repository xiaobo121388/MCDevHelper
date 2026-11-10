import { chromium, expect } from "@playwright/test";
import { createServer } from "node:http";
import { spawn } from "node:child_process";
import { mkdir, readFile, writeFile, mkdtemp } from "node:fs/promises";
import path from "node:path";

const output = path.resolve("target/mcdk-editor-qa");
await mkdir(output, { recursive: true });
const native = process.argv.includes("--native");
const png = await readFile("src-tauri/icons/128x128.png");
const image = "data:image/png;base64," + png.toString("base64");
const errors = [];
const tabs = ["启动", "世界", "Mod 目录", "皮肤", "窗口", "调试"];

async function inspect(page, name) {
  await expect(page.getByRole("heading", { name: "MCDK 可视化配置", exact: true })).toBeVisible();
  for (const tab of tabs) {
    await page.getByRole("navigation", { name: "配置分类" }).getByRole("button", { name: tab, exact: true }).click();
    await expect(page.locator(".mcdk-editor-content section:not([hidden])")).toBeVisible();
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
    expect(await page.locator(".mcdk-editor-content").evaluate(n => n.scrollWidth <= n.clientWidth + 1)).toBe(true);
    if (tab === "皮肤") {
      await expect(page.getByRole("option", { name: "Alex · Slim", exact: true })).toBeAttached();
      await expect(page.locator(".mcdk-skin-preview img")).toBeVisible();
      await expect.poll(() => page.locator(".mcdk-skin-preview img").evaluate(img => img.naturalWidth)).toBeGreaterThan(0);
      expect(await page.locator(".mcdk-skin-preview img").evaluate(img => {
        const canvas = document.createElement("canvas"); canvas.width = canvas.height = 32;
        const ctx = canvas.getContext("2d"); ctx.drawImage(img, 0, 0, 32, 32);
        const pixels = ctx.getImageData(0, 0, 32, 32).data;
        return new Set(Array.from({ length: 1024 }, (_, i) => pixels.slice(i * 4, i * 4 + 4).join(","))).size;
      })).toBeGreaterThan(4);
    }
    await page.screenshot({ path: path.join(output, name + "-" + tabs.indexOf(tab) + ".png") });
  }
}

async function browserTest() {
  const root = path.resolve("dist");
  const server = createServer(async (request, response) => {
    try {
      const url = new URL(request.url, "http://localhost");
      const file = path.resolve(root, url.pathname === "/" ? "index.html" : decodeURIComponent(url.pathname).slice(1));
      if (!file.startsWith(root + path.sep)) { response.writeHead(403).end(); return; }
      const bytes = await readFile(file);
      response.writeHead(200, { "Content-Type": { ".html": "text/html", ".js": "text/javascript", ".css": "text/css", ".png": "image/png" }[path.extname(file)] ?? "application/octet-stream" }).end(bytes);
    } catch { response.writeHead(404).end(); }
  });
  await new Promise(resolve => server.listen(0, "127.0.0.1", resolve));
  const browser = await chromium.launch({ channel: "msedge", headless: true, args: ["--no-proxy-server"] });
  try {
    for (const [name, width, height] of [["desktop", 1000, 760], ["compact", 700, 650], ["mobile", 390, 844]]) {
      for (const theme of ["light", "dark"]) {
        const context = await browser.newContext({ viewport: { width, height } });
        await context.addInitScript(({ theme, image }) => {
          window.__MCDH_DIALOG__ = { kind: "mcdk_editor", componentId: "fixture" };
          window.isTauri = true;
          window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener() {} };
          const callbacks = new Map(); let sequence = 1;
          let snapshot = { path: "D:/Projects/Fixture/.mcdev.json", revision: "1", data: { game_executable_path: "D:/Game/Minecraft.Windows.exe", world_name: "测试世界", skin_info: { skin: "D:/Game/alex.png", slim: true, keep: "metadata" }, included_mod_dirs: ["./", { path: "./addon", enabled: false, hot_reload: true }] } };
          let custom = [];
          window.__TAURI_INTERNALS__ = {
            metadata: { currentWindow: { label: "dialog-mcdk-fixture" }, currentWebview: { label: "dialog-mcdk-fixture" } },
            transformCallback(callback) { const id = sequence++; callbacks.set(id, callback); return id; },
            unregisterCallback(id) { callbacks.delete(id); },
            async invoke(command, args = {}) {
              if (command.startsWith("plugin:event|")) return sequence++;
              if (command === "plugin:window|is_maximized") return false;
              if (command.startsWith("plugin:window|")) return null;
              if (command === "get_settings") return { theme, color_preset: "fluent", developer_nickname: "QA", developer_account: "test", developer_user_id: "0" };
              if (command === "mcdk_status") return { available: true, phase: "idle", auto_update: false };
              if (command === "get_mcdk_config") return structuredClone(snapshot);
              if (command === "save_mcdk_config") {
                if (args.revision !== snapshot.revision) throw new Error("stale revision");
                for (const edit of args.edits) { let target = snapshot.data; for (const part of edit.path.slice(0, -1)) target = target[part] ??= {}; target[edit.path.at(-1)] = edit.value; }
                snapshot.revision = String(Number(snapshot.revision) + 1); window.__qaSaved = structuredClone(snapshot); return structuredClone(snapshot);
              }
              if (command === "get_mcdk_skin_library") return custom;
              if (command === "update_mcdk_skin_library") { custom = args.remove ? custom.filter(s => s.path !== args.path) : [...custom, { path: args.path, slim: args.slim }]; return custom; }
              if (command === "get_mcdk_skin_preview") return image;
              if (command === "get_mcdk_skin_catalog") return { warnings: [], skins: [
                { name: "Steve", path: "D:/Game/steve.png", texture: "steve.png", slim: false, kind: "vanilla" },
                { name: "Alex", path: "D:/Game/alex.png", texture: "alex.png", slim: true, kind: "vanilla" },
                { name: "4D测试皮肤", path: "D:/4D/skin.zip", texture: "skin.zip", slim: false, kind: "netease4d" },
              ] };
              if (command === "launch_component_game") throw new Error("QA must never launch a game");
              return null;
            },
          };
        }, { theme, image });
        const page = await context.newPage(); page.on("pageerror", error => errors.push(error.message));
        await page.goto("http://127.0.0.1:" + server.address().port);
        await inspect(page, name + "-" + theme);
        await page.getByRole("button", { name: "世界", exact: true }).click();
        await page.getByRole("textbox", { name: "世界名称", exact: true }).fill("已修改世界");
        await page.getByRole("button", { name: "保存", exact: true }).click();
        await expect(page.getByText("配置已保存", { exact: true })).toBeVisible();
        expect(await page.evaluate(() => window.__qaSaved.data.skin_info.keep)).toBe("metadata");
        await context.close();
      }
    }
  } finally { await browser.close(); await new Promise(resolve => server.close(resolve)); }
}

async function nativeTest() {
  const data = await mkdtemp(path.join(output, "native-"));
  const game = path.join(data, "games/bin/1.0.0/Minecraft.Windows.exe");
  const vanilla = path.join(path.dirname(game), "data/skin_packs/vanilla");
  await mkdir(vanilla, { recursive: true }); await writeFile(game, "QA fixture: not executable");
  await writeFile(path.join(vanilla, "alex.png"), png); await writeFile(path.join(data, "custom.png"), png);
  await writeFile(path.join(vanilla, "skins.json"), JSON.stringify({ skins: [{ type: "free", localization_name: "Alex", geometry: "geometry.humanoid.customSlim", texture: "alex.png" }] }));
  const endpoint = "http://127.0.0.1:9238";
  let occupied = false;
  try { await fetch(endpoint + "/json/version", { signal: AbortSignal.timeout(500) }); occupied = true; } catch { /* Port is available. */ }
  if (occupied) throw new Error("Port 9238 is occupied; refusing to attach to another WebView.");
  const child = spawn(path.resolve("target/debug/mcdh-desktop.exe"), [], { windowsHide: true, env: { ...process.env,
    MCDH_DATA_DIR: path.join(data, "index"), MCDH_DISABLE_MCS_SCAN: "1", WEBVIEW2_USER_DATA_FOLDER: path.join(data, "webview"),
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: "--remote-debugging-port=9238 --remote-debugging-address=127.0.0.1",
  }, stdio: ["ignore", "pipe", "pipe"] });
  const logs = []; child.stdout.on("data", d => logs.push(d.toString())); child.stderr.on("data", d => logs.push(d.toString()));
  let browser, main, editor;
  try {
    await expect.poll(async () => { if (child.exitCode !== null) throw new Error(logs.join("")); try { return (await fetch(endpoint + "/json/version")).status; } catch { return 0; } }, { timeout: 30000 }).toBe(200);
    browser = await chromium.connectOverCDP(endpoint); const context = browser.contexts()[0];
    await expect.poll(async () => {
      for (const page of context.pages()) if (await page.evaluate(() => window.__TAURI_INTERNALS__?.metadata.currentWindow.label).catch(() => "") === "main") { main = page; return true; }
      return false;
    }, { timeout: 30000 }).toBe(true);
    context.on("page", page => page.on("pageerror", e => errors.push(e.message)));
    await expect(main.getByRole("heading", { name: "全部组件", exact: true })).toBeVisible({ timeout: 30000 });
    const component = await main.evaluate(async destination => {
      const invoke = window.__TAURI_INTERNALS__.invoke;
      await invoke("set_mcdk_auto_update", { enabled: false });
      return (await invoke("create_component", { request: { name: "MCDK Editor QA", kind: "addon", destination, mcs_compatible: false } })).component;
    }, data);
    const configPath = path.join(component.path, ".mcdev.json");
    await writeFile(configPath, "\ufeff{\r\n// QA comment\r\n" + JSON.stringify({ game_executable_path: game, world_name: "QA world", skin_info: { skin: path.join(vanilla, "alex.png"), slim: true, retained: 42 }, extension_field: "keep" }).slice(1, -1) + ",\r\n}\r\n");
    await main.getByRole("button", { name: "刷新组件", exact: true }).click();
    await main.getByRole("button", { name: "MCDK 可视化配置", exact: true }).click();
    await expect.poll(async () => {
      for (const page of context.pages()) if ((await page.evaluate(() => window.__TAURI_INTERNALS__?.metadata.currentWindow.label).catch(() => "")).startsWith("dialog-mcdk-")) { editor = page; return true; }
      return false;
    }, { timeout: 20000 }).toBe(true);
    await inspect(editor, "native");
    await editor.getByRole("button", { name: "世界", exact: true }).click();
    await editor.getByRole("textbox", { name: "世界名称", exact: true }).fill("Native saved world");
    await editor.getByRole("button", { name: "保存", exact: true }).click();
    await expect(editor.getByText("配置已保存", { exact: true })).toBeVisible();
    const saved = await readFile(configPath, "utf8"); expect(saved.startsWith("\ufeff")).toBe(true); expect(saved).toContain("// QA comment"); expect(saved).toContain("Native saved world"); expect(saved).toContain('"retained":42');
    await editor.getByRole("button", { name: "皮肤", exact: true }).click();
    await editor.getByRole("textbox", { name: "添加皮肤路径" }).fill(path.join(data, "custom.png"));
    await editor.getByRole("button", { name: "加入皮肤库", exact: true }).click();
    await expect(editor.getByRole("option", { name: "custom.png", exact: true })).toBeAttached();
    await editor.getByRole("button", { name: "保存", exact: true }).click();
    await expect(editor.getByText("配置已保存", { exact: true })).toBeVisible();
    expect(await main.evaluate(componentId => window.__TAURI_INTERNALS__.invoke("get_mcdk_skin_library", { componentId }), component.id)).toHaveLength(1);
    await editor.getByRole("button", { name: "世界", exact: true }).click();
    await editor.getByRole("textbox", { name: "世界名称", exact: true }).fill("stale must not save");
    await writeFile(configPath, (await readFile(configPath, "utf8")).replace("Native saved world", "External edit"));
    await editor.getByRole("button", { name: "保存", exact: true }).click();
    await expect(editor.getByRole("alert")).toContainText("配置已被其他程序修改");
    expect(await readFile(configPath, "utf8")).toContain("External edit");
    await editor.screenshot({ path: path.join(output, "native-conflict.png") });
    // Exercise actual native confirmation ownership and the close-request guard.
    await editor.getByRole("button", { name: "关闭配置", exact: true }).click();
    let confirmation;
    await expect.poll(async () => {
      for (const page of context.pages()) if ((await page.evaluate(() => window.__TAURI_INTERNALS__?.metadata.currentWindow.label).catch(() => "")).startsWith("dialog-confirm-")) { confirmation = page; return true; }
      return false;
    }).toBe(true);
    await confirmation.getByRole("button", { name: "取消", exact: true }).click();
    await expect(editor.getByRole("textbox", { name: "世界名称", exact: true })).toHaveValue("stale must not save");
    await editor.getByRole("button", { name: "关闭窗口", exact: true }).click();
    confirmation = undefined;
    await expect.poll(async () => {
      for (const page of context.pages()) if ((await page.evaluate(() => window.__TAURI_INTERNALS__?.metadata.currentWindow.label).catch(() => "")).startsWith("dialog-confirm-")) { confirmation = page; return true; }
      return false;
    }).toBe(true);
    await confirmation.getByRole("button", { name: "确认", exact: true }).click();
    await expect.poll(() => editor.isClosed()).toBe(true);
  } finally {
    if (main && !main.isClosed()) await main.evaluate(() => window.__TAURI_INTERNALS__.invoke("plugin:window|close", { label: "main" })).catch(() => {});
    if (browser) await browser.close().catch(() => {});
    if (child.exitCode === null) child.kill();
    await writeFile(path.join(output, "native-host.log"), logs.join(""));
  }
}

if (native) await nativeTest(); else await browserTest();
expect(errors).toEqual([]);
console.log((native ? "Native WebView2" : "Browser multi-viewport") + " MCDK editor validation passed. Artifacts: " + output);
