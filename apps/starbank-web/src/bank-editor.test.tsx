import {
  AppShell,
  type BankEditorDocument,
  type EditorRow,
  normalizeDraft,
  prepareTableDraft,
} from "@starbank/ui";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

const gold: EditorRow = {
  id: "gold",
  name: "Gold",
  type: "int",
  value: "00042",
};
const experience: EditorRow = {
  id: "xp",
  name: "Progress",
  childName: "Experience",
  type: "fixed",
  value: "7.50",
};
const note: EditorRow = {
  id: "note",
  name: "Note",
  type: "string",
  value: "A, B",
};
const document: BankEditorDocument = {
  id: "fixture",
  revision: "1",
  name: "Fixture.SC2Bank",
  xml: "<Bank/>",
  dirty: false,
  sections: [{ id: "player", name: "Player", rows: [gold, experience, note] }],
};

describe("bank editor", () => {
  it("renders only file controls supplied by the adapter", () => {
    const markup = renderToStaticMarkup(
      <AppShell
        name="StarBank Web"
        editor={{ onOpenFile: async () => ({ ok: true }) }}
      />,
    );
    expect(markup).toContain("Open Bank…");
    expect(markup).toContain('accept=".sc2bank,.xml"');
    expect(markup).not.toContain("Save As…");
  });

  it("shows sections, child values, and actions from an actual document projection", () => {
    const markup = renderToStaticMarkup(
      <AppShell
        name="StarBank Web"
        editor={{
          document,
          onEditValues: async () => ({ ok: true }),
          onSaveAs: async () => ({ ok: true }),
          onCopy: async () => ({ ok: true }),
        }}
      />,
    );
    expect(markup).toContain("Fixture.SC2Bank");
    expect(markup).toContain("Sections");
    expect(markup).toContain("Progress · Experience");
    expect(markup).toContain('value="00042"');
    expect(markup).toContain("Save As…");
    expect(markup).not.toContain("Simple View");
  });

  it("normalizes intentional grouping without coercion or rounding", () => {
    expect(normalizeDraft(gold, "9,007,199,254,740,993")).toBe(
      "9007199254740993",
    );
    expect(normalizeDraft(experience, "-1,234.50")).toBe("-1234.50");
    expect(normalizeDraft(gold, "12,34")).toBeNull();
    expect(normalizeDraft(experience, "-1,,234.50")).toBeNull();
    expect(normalizeDraft(note, "A, C")).toBe("A, C");
  });

  it("preserves exact edited strings and identifies invalid targets before apply", () => {
    const result = prepareTableDraft(
      document.sections,
      new Map([
        ["gold", "00042"],
        ["xp", "9,007,199,254,740,993.50"],
        ["note", "C, D"],
        ["removed", "3"],
      ]),
    );
    expect(result.edits).toEqual([
      { id: "xp", value: "9007199254740993.50" },
      { id: "note", value: "C, D" },
    ]);
    expect([...result.invalid]).toEqual(["removed"]);
  });
});
