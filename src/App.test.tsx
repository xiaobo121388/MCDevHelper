import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { App } from "./App";

describe("App", () => {
  it("renders the local-first component workspace", () => {
    render(<App />);
    expect(screen.getByRole("heading", { name: "全部组件" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /新建组件/ })).toBeInTheDocument();
    expect(screen.getByText(/无自动联网/)).toBeInTheDocument();
  });
});
