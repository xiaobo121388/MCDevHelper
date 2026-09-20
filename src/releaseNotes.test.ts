import { describe, expect, it } from "vitest";
import { version } from "../package.json";
import { releaseNotesFor } from "./releaseNotes";

describe("releaseNotesFor", () => {
  it("provides release notes for the packaged version", () => {
    const notes = releaseNotesFor(version);
    expect(notes.length).toBeGreaterThan(1);
    expect(notes.some((note) => note.includes("MCDK"))).toBe(true);
    expect(notes.some((note) => note.includes("自定义导出"))).toBe(true);
  });

  it("normalizes version prefixes and surrounding whitespace", () => {
    expect(releaseNotesFor(" v1.2.0 ")).toEqual(releaseNotesFor("1.2.0"));
    expect(releaseNotesFor("V1.2.0")).toEqual(releaseNotesFor("1.2.0"));
  });

  it("preserves historical notes and handles unknown versions", () => {
    expect(releaseNotesFor("1.1.0")[0]).toContain(".mcdh.json");
    expect(releaseNotesFor("0.0.0")).toEqual(["此版本包含功能改进与问题修复。"]);
  });
});
