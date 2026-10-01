import { describe, expect, it } from "vitest";
import { version } from "../package.json";
import config from "../src-tauri/tauri.conf.json";
import { releaseNotesFor } from "./releaseNotes";

describe("releaseNotesFor", () => {
  it("keeps the frontend and installer versions consistent", () => {
    expect(config.version).toBe(version);
  });

  it("provides release notes for the packaged version", () => {
    const notes = releaseNotesFor(version);
    expect(notes.length).toBeGreaterThan(1);
    expect(notes.some((note) => note.includes("无边框"))).toBe(true);
    expect(notes.some((note) => note.includes("八套配色"))).toBe(true);
    expect(notes.some((note) => note.includes("一键导出"))).toBe(true);
    expect(notes.some((note) => note.includes("自定义导出"))).toBe(true);
    expect(notes.some((note) => note.includes("增量更新"))).toBe(true);
    expect(notes.some((note) => note.includes("包体位置"))).toBe(true);
    expect(notes.some((note) => note.includes("内置窗口"))).toBe(true);
    expect(new Set(notes).size).toBe(notes.length);
  });

  it("normalizes version prefixes and surrounding whitespace", () => {
    expect(releaseNotesFor(" v1.3.0 ")).toEqual(releaseNotesFor("1.3.0"));
    expect(releaseNotesFor("V1.3.0")).toEqual(releaseNotesFor("1.3.0"));
  });

  it("preserves historical notes and handles unknown versions", () => {
    expect(releaseNotesFor("1.2.0")[0]).toContain("MCDK");
    expect(releaseNotesFor("1.1.0")[0]).toContain(".mcdh.json");
    expect(releaseNotesFor("0.0.0")).toEqual(["此版本包含功能改进与问题修复。"]);
  });
});
