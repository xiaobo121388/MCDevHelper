import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { App } from "./App";

describe("App", () => {
  it("renders the local-first component workspace", () => {
    render(<App />);
    expect(screen.getByRole("heading", { name: "全部组件" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /新建组件/ })).toBeInTheDocument();
    expect(screen.getByText("本地工作区")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "最小化" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "最大化" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "关闭窗口" })).toBeInTheDocument();
    expect(screen.queryByText(/启动检查更新/)).not.toBeInTheDocument();
  });
});
