import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({ refresh: vi.fn() }));

vi.mock("./api", () => ({
  desktop: true,
  api: { refresh: mocks.refresh },
  errorMessage: (error: unknown) => String(error),
}));

import { App } from "./App";

describe("component workspace filters", () => {
  it("filters discovered cards by category, search text, and tag", async () => {
    mocks.refresh.mockResolvedValue({
      components: [
        {
          id: "addon-1",
          name: "中文冒险模组",
          kind: "addon",
          path: "D:\\作品\\冒险",
          origin: { kind: "library", source_id: "source" },
          manifests: [],
          tags: ["开发"],
        },
        {
          id: "material-1",
          name: "柔和材质",
          kind: "material",
          path: "D:\\作品\\材质",
          origin: { kind: "single", source_id: "single" },
          manifests: [],
          tags: ["发布"],
        },
      ],
      sources: [],
      warnings: [],
    });
    render(<App />);

    await waitFor(() => expect(screen.getByText("中文冒险模组")).toBeInTheDocument());
    expect(screen.getByText("柔和材质")).toBeInTheDocument();

    const navigation = within(screen.getByRole("navigation", { name: "组件分类" }));
    fireEvent.click(navigation.getByRole("button", { name: /材质/ }));
    expect(screen.queryByText("中文冒险模组")).not.toBeInTheDocument();
    expect(screen.getByText("柔和材质")).toBeInTheDocument();

    fireEvent.click(navigation.getByRole("button", { name: /全部组件/ }));
    fireEvent.change(screen.getByPlaceholderText("搜索名称或路径"), {
      target: { value: "冒险" },
    });
    expect(screen.getByText("中文冒险模组")).toBeInTheDocument();
    expect(screen.queryByText("柔和材质")).not.toBeInTheDocument();

    fireEvent.change(screen.getByPlaceholderText("搜索名称或路径"), {
      target: { value: "" },
    });
    fireEvent.change(screen.getAllByRole("combobox")[0], {
      target: { value: "发布" },
    });
    expect(screen.getByText("柔和材质")).toBeInTheDocument();
    expect(screen.queryByText("中文冒险模组")).not.toBeInTheDocument();
  });
});
