import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
const mocks = vi.hoisted(() => ({ load: vi.fn(), save: vi.fn(), catalog: vi.fn(), preview: vi.fn(), library: vi.fn(), updateLibrary: vi.fn(), launch: vi.fn(), confirm: vi.fn(), close: vi.fn(), open: vi.fn() }));
vi.mock("./mcdkEditorApi", async importOriginal => ({ ...await importOriginal<typeof import("./mcdkEditorApi")>(), editorApi: mocks }));
vi.mock("./api", () => ({ api: { launchGame: mocks.launch }, errorMessage: (e: unknown) => e instanceof Error ? e.message : String(e) }));
vi.mock("./windows", () => ({ nativeWindows: false, confirmAction: mocks.confirm }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: mocks.open }));
import { McdkEditor } from "./McdkEditor";
import { applyEdits, legacyVanillaMatch, type ConfigEdit, type Skin } from "./mcdkEditorApi";

const initial = { path: "D:/Project/.mcdev.json", revision: "one", data: { world_name: "Test world", skin_info: { skin: "", slim: false, custom: 42 }, unknown: { retained: true } } };
const alex: Skin = { name: "Alex", path: "D:/Game/alex.png", texture: "alex.png", kind: "vanilla", slim: true };
const four: Skin = { name: "4D测试皮肤", path: "D:/support/v2025s5_ceshi/4680777111329095206.zip", texture: "4d", kind: "netease4d", slim: false };
const mount = () => render(<McdkEditor componentId="test" onClose={mocks.close} canLaunch />);
const ready = async () => screen.findByRole("textbox", { name: "玩家名称" });
const tab = (name: string) => fireEvent.click(screen.getByRole("button", { name }));

describe("MCDK visual configuration", () => {
  beforeEach(() => {
    vi.resetAllMocks(); mocks.load.mockResolvedValue(structuredClone(initial));
    mocks.library.mockResolvedValue([]); mocks.catalog.mockResolvedValue({ skins: [alex, four], warnings: [] });
    mocks.preview.mockResolvedValue("data:image/png;base64,fixture"); mocks.confirm.mockResolvedValue(true);
    mocks.save.mockImplementation(async (_id: string, _revision: string, edits: ConfigEdit[]) => ({ ...initial, revision: "two", data: applyEdits(initial.data, edits) }));
  });
  afterEach(cleanup);

  it("does not write or browse on load, and sends only changed fields", async () => {
    mount(); await ready(); expect(mocks.save).not.toHaveBeenCalled(); expect(mocks.open).not.toHaveBeenCalled();
    tab("世界"); fireEvent.change(screen.getByRole("textbox", { name: "世界名称" }), { target: { value: "Changed" } });
    fireEvent.click(screen.getByRole("button", { name: "保存" }));
    await screen.findByText("配置已保存");
    expect(mocks.save).toHaveBeenCalledWith("test", "one", [{ path: ["world_name"], value: "Changed" }]);
    expect(screen.getByRole("button", { name: "保存" })).toBeDisabled();
  });
  it("retains edits and does not launch when the revision is stale", async () => {
    mocks.save.mockRejectedValue(new Error("配置已被其他程序修改")); mount(); await ready();
    fireEvent.change(screen.getByRole("textbox", { name: "玩家名称" }), { target: { value: "Edited" } });
    fireEvent.click(screen.getByRole("button", { name: "保存并启动" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("配置已被其他程序修改");
    expect(screen.getByRole("textbox", { name: "玩家名称" })).toHaveValue("Edited"); expect(mocks.launch).not.toHaveBeenCalled();
  });
  it("saves before launch and verifies a clean revision on launch", async () => {
    mount(); await ready(); fireEvent.click(screen.getByRole("button", { name: "保存并启动" }));
    await waitFor(() => expect(mocks.launch).toHaveBeenCalledWith("test"));
    expect(mocks.save).toHaveBeenCalledWith("test", "one", []);
    expect(mocks.save.mock.invocationCallOrder[0]).toBeLessThan(mocks.launch.mock.invocationCallOrder[0]);
  });
  it("prompts before discarding unsaved edits or reloading", async () => {
    mocks.confirm.mockResolvedValue(false); mount(); await ready();
    fireEvent.change(screen.getByRole("textbox", { name: "玩家名称" }), { target: { value: "Edited" } });
    fireEvent.click(screen.getByRole("button", { name: "关闭配置" })); await waitFor(() => expect(mocks.confirm).toHaveBeenCalledTimes(1)); expect(mocks.close).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "重新加载配置" })); await waitFor(() => expect(mocks.confirm).toHaveBeenCalledTimes(2)); expect(mocks.load).toHaveBeenCalledTimes(1);
  });
  it("selects vanilla slim and 4D without dropping extra skin fields", async () => {
    const source = { ...initial, data: { ...initial.data, game_executable_path: "D:/Game/Minecraft.Windows.exe" } };
    mocks.load.mockResolvedValue(source);
    mocks.save.mockImplementation(async (_id, _revision, edits) => ({ ...source, revision: "two", data: applyEdits(source.data, edits) }));
    mount(); await ready(); tab("皮肤"); await screen.findByRole("option", { name: "Alex · Slim" });
    fireEvent.change(screen.getByRole("combobox", { name: "皮肤选择" }), { target: { value: alex.path } });
    fireEvent.click(screen.getByRole("button", { name: "保存" })); await screen.findByText("配置已保存");
    const changes = mocks.save.mock.calls[0][2]; expect(applyEdits(initial.data, changes).skin_info).toEqual({ skin: alex.path, slim: true, custom: 42 });
    fireEvent.change(screen.getByRole("combobox", { name: "皮肤选择" }), { target: { value: four.path } });
    fireEvent.click(screen.getByRole("button", { name: "保存" }));
    await waitFor(() => expect(mocks.save).toHaveBeenCalledTimes(2));
    expect(mocks.save.mock.calls[1][2]).toContainEqual({ path: ["skin_info", "skin"], value: four.path });
  });
  it("cancelling PNG selection changes neither config nor library", async () => {
    mocks.open.mockResolvedValue(null); mount(); await ready(); tab("皮肤");
    fireEvent.click(screen.getByRole("button", { name: "选择 PNG 皮肤" })); await waitFor(() => expect(mocks.open).toHaveBeenCalled());
    expect(mocks.updateLibrary).not.toHaveBeenCalled(); expect(mocks.save).not.toHaveBeenCalled();
  });
  it("matches Windows slash and case variants without rewriting the source", async () => {
    mocks.load.mockResolvedValue({ ...initial, data: { game_executable_path: "D:/Game/Minecraft.Windows.exe", skin_info: { skin: "d:\\game\\ALEX.png", slim: true } } });
    mount(); await ready(); tab("皮肤"); await screen.findByRole("option", { name: "Alex · Slim" });
    expect(screen.getByRole("combobox", { name: "皮肤选择" })).toHaveValue(alex.path);
    expect(screen.getByRole("textbox", { name: "皮肤路径" })).toHaveValue("d:\\game\\ALEX.png");
    expect(mocks.save).not.toHaveBeenCalled();
  });
  it("rejects stale asynchronous preview responses", async () => {
    let finishFirst!: (value: string) => void;
    mocks.preview.mockImplementation((_id: string, path: string) => path === "first.png" ? new Promise(resolve => { finishFirst = resolve; }) : Promise.resolve("data:image/png;base64,second"));
    mount(); await ready(); tab("皮肤"); const input = screen.getByRole("textbox", { name: "皮肤路径" });
    fireEvent.change(input, { target: { value: "first.png" } }); await waitFor(() => expect(mocks.preview).toHaveBeenCalledWith("test", "first.png"));
    fireEvent.change(input, { target: { value: "second.png" } }); await waitFor(() => expect(screen.getByRole("img")).toHaveAttribute("src", "data:image/png;base64,second"));
    await act(async () => finishFirst("data:image/png;base64,stale"));
    expect(screen.getByRole("img")).toHaveAttribute("src", "data:image/png;base64,second");
  });
  it("persists custom skin model changes back to the workspace library", async () => {
    mocks.load.mockResolvedValue({ ...initial, data: { skin_info: { skin: "D:/custom.png", slim: false } } });
    mocks.library.mockResolvedValue([{ path: "D:/custom.png", slim: false }]);
    mocks.updateLibrary.mockResolvedValue([{ path: "D:/custom.png", slim: true }]);
    mount(); await ready(); tab("皮肤"); await screen.findByRole("option", { name: "custom.png" });
    fireEvent.click(screen.getByRole("checkbox", { name: "Slim 模型（三像素手臂）" }));
    await waitFor(() => expect(mocks.updateLibrary).toHaveBeenCalledWith("test", "D:/custom.png", true, false));
    expect(screen.getByRole("checkbox", { name: "Slim 模型（三像素手臂）" })).toBeChecked();
  });
  it("preserves additional mod fields when toggling a directory", async () => {
    mocks.load.mockResolvedValue({ ...initial, data: { included_mod_dirs: [{ path: "./", enabled: true, hot_reload: true, extra: 7 }] } });
    mount(); await ready(); tab("Mod 目录"); fireEvent.click(screen.getByRole("checkbox", { name: "热重载" }));
    fireEvent.click(screen.getByRole("button", { name: "保存" })); await screen.findByText("配置已保存");
    expect(mocks.save.mock.calls[0][2][0].value[0]).toEqual({ path: "./", enabled: true, hot_reload: false, extra: 7 });
  });
  it("shows running configurations read-only", async () => {
    render(<McdkEditor componentId="test" onClose={mocks.close} running canLaunch />); await ready();
    expect(screen.getByRole("textbox", { name: "玩家名称" })).toBeDisabled(); expect(screen.getByRole("button", { name: "保存并启动" })).toBeDisabled();
  });
  it("initializes a new config only on explicit save", async () => {
    mocks.load.mockResolvedValue({ ...initial, data: {}, revision: null }); mount(); await ready(); expect(mocks.save).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "保存" })); await screen.findByText("配置已保存");
    expect(mocks.save.mock.calls[0][1]).toBeNull(); expect(mocks.save.mock.calls[0][2]).toContainEqual({ path: ["included_mod_dirs"], value: ["./"] });
  });
  it("matches old vanilla versions only within the same installation tree", () => {
    const game = "D:/games/3.10.0/Minecraft.Windows.exe";
    expect(legacyVanillaMatch("D:/games/3.9.0/data/skin_packs/vanilla/alex.png", game, [alex])).toEqual(alex);
    expect(legacyVanillaMatch("D:/other/3.9.0/data/skin_packs/vanilla/alex.png", game, [alex])).toBeUndefined();
  });
});
