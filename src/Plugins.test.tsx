import { act, cleanup, fireEvent, render, renderHook, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { PluginCandidate, PluginPanelInfo, PluginSummary } from "./pluginTypes";

const mocks = vi.hoisted(() => ({ list:vi.fn(), builtins:vi.fn(), installBuiltin:vi.fn(), preview:vi.fn(), previewLocal:vi.fn(), pick:vi.fn(), install:vi.fn(), discard:vi.fn(), configure:vi.fn(), uninstall:vi.fn(), execute:vi.fn(), cancel:vi.fn(), panel:vi.fn(), panelCall:vi.fn(), logs:vi.fn(), load:vi.fn(), open:vi.fn(), confirm:vi.fn(), notice:vi.fn(), onPanel:vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({open:mocks.pick}));
vi.mock("./pluginApi", () => ({ pluginDesktop:true, pluginApi:mocks, pluginAssetUrl:(_request: unknown, path: string) => `http://mcdh-plugin.localhost/test/${path}` }));
vi.mock("./pluginModules", () => ({ loadPluginModule:mocks.load }));
vi.mock("./windows", () => ({ openDialogWindow:mocks.open, confirmAction:mocks.confirm, notifyWorkspace:vi.fn().mockResolvedValue(undefined) }));
vi.mock("./App", () => ({ Modal:({ title, children }: { title:string; children:React.ReactNode }) => <section role="dialog" aria-label={title}>{children}</section> }));
import { PluginManager } from "./PluginManager";
import { PluginPanelHost } from "./PluginPanelHost";
import { PluginCommandPrompt } from "./PluginForms";
import { usePlugins } from "./usePlugins";
import type { McdkController } from "./Mcdk";
import { createContext } from "../sdk/context.js";

const plugin: PluginSummary = { manifest:{ schema_version:1, api_version:1, id:"example.hello", name:"测试插件", version:"1.0.0", description:"测试组件工具", author:"Example", engines:{mcdh:">=1.0.0"}, entry:"main.js",
  commands:[{id:"inspect",title:"读取组件",description:"Inspect",menus:["global","component"],input_schema:{type:"object",properties:{name:{type:"string",title:"名称"}},required:["name"]}}], panels:[{id:"main",title:"组件概览",entry:"panel.js"}], mcp_tools:[{name:"inspect",command:"inspect",description:"Inspect"}], settings_schema:{type:"object",properties:{label:{type:"string",title:"显示名称",default:"hello"}}} },
  repository:"owner/repo",source:{kind:"release",reference:"v1.0.0",archive_sha256:"a".repeat(64)},fingerprint:"b".repeat(64),enabled:true,trusted:true,compatible:true,allow_mcp:false,settings:{label:"hello"} };
const candidate: PluginCandidate = { token:"preview",manifest:plugin.manifest,repository:plugin.repository,source:plugin.source,fingerprint:plugin.fingerprint,compatible:true,already_installed:false };
const request = { pluginId:plugin.manifest.id, panelId:"main", fingerprint:plugin.fingerprint, componentId:"component-1" };
const panelInfo: PluginPanelInfo = { plugin,panel:plugin.manifest.panels[0],environment:{caller:"desktop",pluginId:plugin.manifest.id,appVersion:"1.3.0",dataDir:"D:/data",packageDir:"D:/package",componentId:"component-1"} };

function Manager() { const controller = usePlugins(mocks.notice, mocks.onPanel); return <PluginManager controller={controller} />; }
beforeEach(() => {
  vi.clearAllMocks();
  mocks.list.mockResolvedValue([plugin]); mocks.preview.mockResolvedValue(candidate); mocks.install.mockResolvedValue(plugin);
  mocks.builtins.mockResolvedValue([]); mocks.installBuiltin.mockResolvedValue(plugin);
  mocks.discard.mockResolvedValue(undefined); mocks.configure.mockResolvedValue(plugin); mocks.uninstall.mockResolvedValue(undefined);
  mocks.cancel.mockResolvedValue(undefined); mocks.panel.mockResolvedValue(panelInfo); mocks.panelCall.mockResolvedValue({value:null,actions:[]});
  mocks.execute.mockResolvedValue({value:{ok:true},actions:[]}); mocks.logs.mockResolvedValue([]); mocks.confirm.mockResolvedValue(true);
  mocks.open.mockResolvedValue(undefined);
  mocks.pick.mockResolvedValue(null); mocks.previewLocal.mockResolvedValue({...candidate,repository:"Local import",source:{...candidate.source,kind:"local_zip",reference:"D:/plugin.zip"}});
});
afterEach(() => { cleanup(); document.documentElement.removeAttribute("style"); });

describe("plugin management", () => {
  it.each([false,true])("toggles the icon-only enable switch (enabled=%s)", async (enabled) => {
    const current = {...plugin,enabled,allow_mcp:enabled};
    const next = {...current,enabled:!enabled,allow_mcp:false};
    mocks.list.mockResolvedValue([current]);
    mocks.configure.mockImplementationOnce(async () => { mocks.list.mockResolvedValue([next]); return next; });
    render(<Manager />);
    const toggle = await screen.findByRole("switch", {name:"启用 测试插件"});
    expect(toggle).toHaveProperty("checked", enabled);
    expect(toggle.parentElement).toHaveAttribute("title", "启用 测试插件");
    expect(toggle.parentElement?.textContent).toBe("");
    fireEvent.click(toggle);
    await waitFor(() => expect(mocks.configure).toHaveBeenCalledWith(current,!enabled,false,current.settings));
    await waitFor(() => expect(toggle).toHaveProperty("checked", !enabled));
  });
  it.each([{trusted:false},{compatible:false}])("disables the enable switch for unavailable plugins (%j)", async (state) => {
    mocks.list.mockResolvedValue([{...plugin,...state}]);
    render(<Manager />);
    expect(await screen.findByRole("switch", {name:"启用 测试插件"})).toBeDisabled();
    expect(mocks.configure).not.toHaveBeenCalled();
  });
  it("uses an icon-only uninstall button with an accessible name and tooltip", async () => {
    render(<Manager />);
    const uninstall = await screen.findByRole("button", {name:"卸载 测试插件"});
    expect(uninstall).toHaveClass("icon-button", "plugin-uninstall");
    expect(uninstall).toHaveAttribute("title", "卸载 测试插件");
    expect(uninstall.textContent).toBe("");
    expect(uninstall.querySelector("svg")).toBeInTheDocument();
  });
  it("uninstalls MCDE with confirmation and removes its registration from the page", async () => {
    const builtin: PluginSummary = {...plugin,manifest:{...plugin.manifest,id:"mcdh.mcde",name:"MCDE",panels:[],mcp_tools:[],settings_schema:{type:"object"}},source:{...plugin.source,kind:"builtin"}};
    mocks.list.mockResolvedValue([builtin]);
    render(<Manager />);
    fireEvent.click(await screen.findByRole("switch", {name:"启用 MCDE"}));
    await waitFor(() => expect(mocks.configure).toHaveBeenCalledWith(builtin,false,false,builtin.settings));
    fireEvent.click(screen.getByRole("button", {name:"卸载 MCDE"}));
    expect(screen.getByRole("checkbox", {name:/同时清除插件管理数据/})).not.toBeChecked();
    expect(mocks.uninstall).not.toHaveBeenCalled();
    mocks.uninstall.mockImplementation(async () => { mocks.list.mockResolvedValue([]); });
    fireEvent.click(screen.getByRole("button", {name:"确认卸载"}));
    await waitFor(() => expect(mocks.uninstall).toHaveBeenCalledWith("mcdh.mcde",false));
    await waitFor(() => expect(screen.queryByRole("switch", {name:"启用 MCDE"})).not.toBeInTheDocument());
    expect(document.body.textContent).not.toContain("MCDE");
    expect(screen.queryByRole("button", {name:"检查更新 MCDE"})).not.toBeInTheDocument();
    expect(screen.queryByRole("checkbox", {name:"允许 MCP 调用 MCDE"})).not.toBeInTheDocument();
  });
  it("exposes MCDE directory settings on the plugin page and protects an active editor", async () => {
    const builtin: PluginSummary = {...plugin,manifest:{...plugin.manifest,id:"mcdh.mcde",name:"MCDE",panels:[],mcp_tools:[],settings_schema:{type:"object"}},source:{...plugin.source,kind:"builtin"}};
    mocks.list.mockResolvedValue([builtin]);
    const mcdk = {status:{session:{mode:"editor"},mcde:{available:true,host_available:true,directory:"D:/MCDE",plugin_version:"0.2.0"}},launching:null,error:"",update:vi.fn()} as unknown as McdkController;
    function NativeManager() { const controller = usePlugins(mocks.notice,mocks.onPanel); return <PluginManager controller={controller} mcdk={mcdk} />; }
    render(<NativeManager />);
    expect(await screen.findByRole("switch", {name:"启用 MCDE"})).toBeDisabled();
    expect(screen.getByRole("button", {name:"卸载 MCDE"})).toBeDisabled();
    expect(screen.getByRole("textbox", {name:"MCDE 插件目录"})).toHaveValue("D:/MCDE");
    expect(screen.getByRole("button", {name:"选择 MCDE 插件目录"})).toBeDisabled();
  });
  it("only shows an uninstalled native plugin after explicitly opening the catalog", async () => {
    const builtin: PluginSummary = {...plugin,manifest:{...plugin.manifest,id:"mcdh.mcde",name:"MCDE",panels:[],mcp_tools:[],settings_schema:{type:"object"}},source:{...plugin.source,kind:"builtin"}};
    mocks.list.mockResolvedValue([]); mocks.builtins.mockResolvedValue([builtin.manifest]);
    mocks.installBuiltin.mockImplementation(async () => { mocks.list.mockResolvedValue([builtin]); return builtin; });
    render(<Manager />);
    expect(document.body.textContent).not.toContain("MCDE");
    expect(mocks.builtins).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", {name:"内置插件"}));
    const install = await screen.findByRole("button", {name:"安装 MCDE"});
    expect(mocks.installBuiltin).not.toHaveBeenCalled();
    fireEvent.click(install);
    expect(await screen.findByRole("switch", {name:"启用 MCDE"})).toBeChecked();
    expect(mocks.installBuiltin).toHaveBeenCalledWith("mcdh.mcde");
    expect(screen.queryByRole("region", {name:"内置插件列表"})).not.toBeInTheDocument();
  });
  it("does not restore a removed plugin from a late list response", async () => {
    const {result} = renderHook(() => usePlugins(mocks.notice,mocks.onPanel));
    await waitFor(() => expect(result.current.items).toHaveLength(1));
    let resolve!: (items: PluginSummary[]) => void;
    mocks.list.mockImplementationOnce(() => new Promise<PluginSummary[]>((done) => { resolve = done; }));
    let pending!: Promise<void>;
    act(() => { pending = result.current.reload(); });
    mocks.list.mockResolvedValue([]);
    await act(async () => { await result.current.reload(); });
    await act(async () => { resolve([plugin]); await pending; });
    expect(result.current.items).toEqual([]);
  });
  it("removes an uninstalled plugin even when the follow-up refresh fails", async () => {
    render(<Manager />);
    fireEvent.click(await screen.findByRole("button", {name:"卸载 测试插件"}));
    mocks.uninstall.mockImplementation(async () => { mocks.list.mockRejectedValue(new Error("Registry unavailable")); });
    fireEvent.click(screen.getByRole("button", {name:"确认卸载"}));
    await waitFor(() => expect(screen.queryByRole("button", {name:"卸载 测试插件"})).not.toBeInTheDocument());
    expect(await screen.findByRole("alert")).toHaveTextContent("Registry unavailable");
  });
  it.each([false,true])("previews a local import and still requires trust (directory=%s)", async (directory) => {
    mocks.pick.mockResolvedValue("D:/plugin.zip");
    render(<Manager />);
    fireEvent.click(screen.getByRole("button", {name:directory ? "导入插件目录" : "本地导入"}));
    expect(await screen.findByText("D:/plugin.zip", {exact:false})).toBeInTheDocument();
    expect(mocks.previewLocal).toHaveBeenCalledWith("D:/plugin.zip",expect.any(String));
    expect(mocks.pick).toHaveBeenCalledWith(expect.objectContaining(directory ? {directory:true} : {filters:[{name:"MCDH 插件包",extensions:["zip"]}]}));
    expect(mocks.preview).not.toHaveBeenCalled(); expect(mocks.install).not.toHaveBeenCalled();
    expect(screen.getByRole("button", {name:"确认安装并启用"})).toBeDisabled();
  });
  it("does not start a task when the local picker is cancelled", async () => {
    render(<Manager />);
    fireEvent.click(screen.getByRole("button", {name:"本地导入"}));
    await waitFor(() => expect(screen.getByRole("button", {name:"本地导入"})).toBeEnabled());
    expect(mocks.previewLocal).not.toHaveBeenCalled();
  });
  it("uses the local picker for local updates and rejects a different plugin id", async () => {
    mocks.list.mockResolvedValue([{...plugin,source:{...plugin.source,kind:"local_zip"}}]);
    mocks.pick.mockResolvedValue("D:/plugin.zip");
    mocks.previewLocal.mockResolvedValue({...candidate,manifest:{...plugin.manifest,id:"other.id"}});
    render(<Manager />);
    fireEvent.click(await screen.findByRole("button", {name:"检查更新 测试插件"}));
    expect(await screen.findByRole("alert")).toHaveTextContent("插件 ID 已变化");
    expect(mocks.discard).toHaveBeenCalledWith("preview"); expect(mocks.preview).not.toHaveBeenCalled();
  });
  it("clears the repository without submitting and only reports busy during a task", async () => {
    render(<Manager />);
    const input = screen.getByLabelText("GitHub 插件仓库");
    const add = screen.getByRole("button", {name:"添加插件"});
    expect(add).toBeDisabled(); expect(add).toHaveAttribute("aria-busy", "false");
    fireEvent.change(input, {target:{value:"owner/repo"}});
    fireEvent.click(screen.getByRole("button", {name:"清空插件仓库"}));
    expect(input).toHaveValue(""); expect(input).toHaveFocus();
    expect(mocks.preview).not.toHaveBeenCalled();
    let reject!: (cause: Error) => void;
    mocks.preview.mockReturnValue(new Promise((_resolve, fail) => { reject = fail; }));
    fireEvent.change(input, {target:{value:"owner/repo"}});
    fireEvent.click(add);
    await waitFor(() => expect(add).toHaveAttribute("aria-busy", "true"));
    await act(async () => { reject(new Error("offline")); });
    expect(add).toBeEnabled(); expect(add).toHaveAttribute("aria-busy", "false");
  });
  it("never installs before a preview and explicit trust confirmation", async () => {
    render(<Manager />);
    fireEvent.change(screen.getByLabelText("GitHub 插件仓库"), {target:{value:"owner/repo"}});
    fireEvent.click(screen.getByRole("button", {name:"添加插件"}));
    const install = await screen.findByRole("button", {name:"确认安装并启用"});
    expect(install).toBeDisabled(); expect(mocks.install).not.toHaveBeenCalled();
    expect(screen.getByText(/当前 Windows 用户权限/)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("checkbox", {name:/我信任此插件/}));
    fireEvent.click(install);
    await waitFor(() => expect(mocks.install).toHaveBeenCalledWith("preview", plugin.fingerprint));
  });
  it("shows incompatible previews without offering an executable install", async () => {
    mocks.preview.mockResolvedValue({...candidate,compatible:false});
    render(<Manager />);
    fireEvent.change(screen.getByLabelText("GitHub 插件仓库"), {target:{value:"owner/repo"}});
    fireEvent.click(screen.getByRole("button", {name:"添加插件"}));
    await screen.findByText(/不兼容当前/);
    fireEvent.click(screen.getByRole("checkbox", {name:/我信任此插件/}));
    expect(screen.getByRole("button", {name:"确认安装并启用"})).toBeDisabled();
  });
  it("requires a separate version-bound MCP confirmation and permits revocation", async () => {
    render(<Manager />);
    fireEvent.click(await screen.findByRole("checkbox", {name:"允许 MCP 调用 测试插件"}));
    await waitFor(() => expect(mocks.confirm).toHaveBeenCalledWith(expect.stringContaining("删除文件")));
    await waitFor(() => expect(mocks.configure).toHaveBeenCalledWith(plugin,true,true,plugin.settings));
    mocks.configure.mockClear(); mocks.confirm.mockResolvedValue(false);
    fireEvent.click(screen.getByRole("checkbox", {name:"允许 MCP 调用 测试插件"}));
    await waitFor(() => expect(mocks.confirm).toHaveBeenCalledTimes(2));
    expect(mocks.configure).not.toHaveBeenCalled();
  });
  it("opens the same declared panel in main or independent surfaces", async () => {
    render(<Manager />);
    fireEvent.click(await screen.findByRole("button", {name:"组件概览"}));
    expect(mocks.onPanel).toHaveBeenCalledWith({pluginId:plugin.manifest.id,panelId:"main",fingerprint:plugin.fingerprint});
    fireEvent.click(screen.getByRole("button", {name:"独立窗口打开 组件概览"}));
    expect(mocks.open).toHaveBeenCalledWith({kind:"plugin_panel",pluginId:plugin.manifest.id,panelId:"main",fingerprint:plugin.fingerprint});
  });
  it("preserves plugin data unless the user explicitly chooses to clear it", async () => {
    render(<Manager />);
    fireEvent.click(await screen.findByRole("button", {name:"卸载 测试插件"}));
    expect(screen.getByRole("checkbox", {name:/同时清除/})).not.toBeChecked();
    expect(mocks.uninstall).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", {name:"确认卸载"}));
    await waitFor(() => expect(mocks.uninstall).toHaveBeenCalledWith(plugin.manifest.id,false));
  });
  it("collects command input and retains the component context", async () => {
    function Prompt() {
      const controller = usePlugins(mocks.notice,mocks.onPanel);
      return <><button onClick={() => controller.requestCommand(plugin,plugin.manifest.commands[0],"component-1")}>run</button><PluginCommandPrompt controller={controller} /></>;
    }
    render(<Prompt />);
    fireEvent.click(screen.getByText("run"));
    fireEvent.change(screen.getByLabelText("名称 *"), {target:{value:"example"}});
    fireEvent.click(screen.getByRole("button", {name:"执行"}));
    await waitFor(() => expect(mocks.execute).toHaveBeenCalledWith(plugin.manifest.id,plugin.fingerprint,"inspect",{name:"example"},"component-1",expect.any(String)));
  });
});

describe("plugin panel host", () => {
  it("attempts every subscription even when one cleanup fails", () => {
    const state = createContext(vi.fn(), {commands:[],environment:panelInfo.environment}, vi.fn());
    const first = vi.fn(); const last = vi.fn();
    state.context.subscriptions.push(first, () => { throw new Error("cleanup failure"); }, last);
    expect(() => state.dispose()).toThrow("cleanup failure");
    expect(first).toHaveBeenCalledOnce(); expect(last).toHaveBeenCalledOnce();
    expect(() => state.dispose()).not.toThrow();
  });
  it("cleans subscriptions immediately when mounting fails", async () => {
    const stop = vi.fn();
    mocks.load.mockResolvedValue({mount:(_element: HTMLElement, context: import("../sdk/mcdh").PanelContext) => {
      context.subscriptions.push(stop); throw new Error("mount failure");
    }});
    const view = render(<PluginPanelHost request={request} onOpenPanel={mocks.onPanel} onClose={vi.fn()} />);
    expect(await screen.findByRole("alert")).toHaveTextContent("mount failure");
    expect(stop).toHaveBeenCalledOnce(); view.unmount(); expect(stop).toHaveBeenCalledOnce();
  });
  it("does not clear a newer panel when an old module load fails late", async () => {
    let rejectOld!: (reason: Error) => void;
    mocks.load.mockImplementationOnce(() => new Promise((_resolve, reject) => { rejectOld = reject; }));
    const view = render(<PluginPanelHost request={request} onOpenPanel={mocks.onPanel} onClose={vi.fn()} />);
    await waitFor(() => expect(mocks.load).toHaveBeenCalledOnce());
    mocks.load.mockResolvedValue({mount:(element: HTMLElement) => { element.textContent = "new panel"; }});
    view.rerender(<PluginPanelHost request={{...request,componentId:"component-2"}} onOpenPanel={mocks.onPanel} onClose={vi.fn()} />);
    const content = () => view.container.querySelector(".plugin-panel-mount")?.shadowRoot?.textContent;
    await waitFor(() => expect(content()).toContain("new panel"));
    await act(async () => { rejectOld(new Error("stale failure")); });
    expect(content()).toContain("new panel");
  });
  it("mounts in Shadow DOM, forwards context/theme, and disposes on close", async () => {
    const stop = vi.fn();
    const listener = vi.fn();
    const mount = vi.fn((element: HTMLElement, context: import("../sdk/mcdh").PanelContext) => {
      element.textContent = "loaded panel"; context.onThemeChanged(listener); return stop;
    });
    mocks.load.mockResolvedValue({mount});
    const view = render(<PluginPanelHost request={request} onOpenPanel={mocks.onPanel} onClose={vi.fn()} />);
    await waitFor(() => expect(mount).toHaveBeenCalledOnce());
    expect(mount.mock.calls[0][1].environment.componentId).toBe("component-1");
    expect(view.container.querySelector(".plugin-panel-mount")?.shadowRoot?.textContent).toContain("loaded panel");
    await act(async () => { document.documentElement.style.colorScheme = "dark"; });
    await waitFor(() => expect(listener).toHaveBeenCalledWith(expect.objectContaining({mode:"dark"})));
    view.unmount(); expect(stop).toHaveBeenCalledOnce();
  });
  it("shows module failures and can reopen an independent panel", async () => {
    mocks.load.mockRejectedValue(new Error("broken module"));
    render(<PluginPanelHost request={request} onOpenPanel={mocks.onPanel} onClose={vi.fn()} />);
    expect(await screen.findByRole("alert")).toHaveTextContent("broken module");
    fireEvent.click(screen.getByRole("button", {name:"在独立窗口打开"}));
    expect(mocks.open).toHaveBeenCalledWith({kind:"plugin_panel",...request});
  });
});
