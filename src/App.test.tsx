import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { App } from "./App";

describe("App", () => {
  it("renders the offline workspace placeholder", () => {
    render(<App />);
    expect(screen.getByRole("heading", { name: "组件管理，简单一点" })).toBeInTheDocument();
  });
});

