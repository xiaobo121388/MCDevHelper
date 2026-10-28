import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
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
    for (const topic of ["GitHub", "本地导入", "面板", "MCP", "设置", "开关", "清空", "全权限"]) {
      expect(notes.some((note) => note.includes(topic))).toBe(true);
    }
    expect(new Set(notes).size).toBe(notes.length);
  });

  it("normalizes version prefixes and surrounding whitespace", () => {
    expect(releaseNotesFor(` v${version} `)).toEqual(releaseNotesFor(version));
    expect(releaseNotesFor(`V${version}`)).toEqual(releaseNotesFor(version));
  });

  it("keeps the Rust workspace and locked application packages on the release version", () => {
    const cargo = readFileSync("Cargo.toml", "utf8").replace(/\r\n/g, "\n");
    const lock = readFileSync("Cargo.lock", "utf8").replace(/\r\n/g, "\n");
    expect(cargo).toContain(`[workspace.package]\nversion = "${version}"`);
    for (const name of ["mcdh-core", "mcdh-desktop", "mcdh-mcp", "mcdh-plugins"]) {
      expect(lock).toContain(`name = "${name}"\nversion = "${version}"`);
    }
  });

  it("preserves historical notes and handles unknown versions", () => {
    expect(releaseNotesFor("1.3.0").some((note) => note.includes("八套配色"))).toBe(true);
    expect(releaseNotesFor("1.2.0")[0]).toContain("MCDK");
    expect(releaseNotesFor("1.1.0")[0]).toContain(".mcdh.json");
    expect(releaseNotesFor("0.0.0")).toEqual(["此版本包含功能改进与问题修复。"]);
  });
});
