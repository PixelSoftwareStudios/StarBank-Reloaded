import { renderToStaticMarkup } from "react-dom/server";
import { defaultSettings } from "starbank-platform/settings";
import { AppShell, type BankEditorDocument, type EditorRow } from "starbank-ui";
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

  it("groups display values without losing precision or lexical padding", () => {
    const rows = [
      gold,
      { ...experience, value: "-9007199254740993.500" },
      { ...note, value: "00042" },
    ];
    const markup = renderToStaticMarkup(
      <AppShell
        name="StarBank Web"
        preferences={{ settings: defaultSettings(), change() {} }}
        editor={{
          document: {
            ...document,
            sections: [{ id: "player", name: "Player", rows }],
          },
          onEditValues: async () => ({ ok: true }),
        }}
      />,
    );
    expect(markup).toContain('value="00,042"');
    expect(markup).toContain('value="-9,007,199,254,740,993.500"');
    expect(markup).toContain('value="00042"');
  });
});
