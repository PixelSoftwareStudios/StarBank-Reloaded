import { renderToStaticMarkup } from "react-dom/server";
import { AppShell, type BankEditorController } from "starbank-ui";
import { decodeEngineReply, engineRequest } from "starbank-ui/engine/protocol";
import { describe, expect, it, vi } from "vitest";

const target = { document_id: "document-1", expected_revision: "14" };
const replyScope = {
  schema_version: 1,
  request_id: "test",
  document_id: target.document_id,
  revision: target.expected_revision,
};
const matched = {
  status: "matched",
  profile_id: "fixture-only",
  confidence: "identity_and_schema",
  groups: [
    {
      label: "Progress",
      description: null,
      fields: [
        {
          id: "score",
          target_id: "11",
          label: "Score",
          description: "Earned points",
          kind: "number",
          value: "9007199254740993.50",
          checked: null,
          read_only: false,
        },
        {
          id: "bonus",
          target_id: "12",
          label: "Calculated bonus",
          description: null,
          kind: "number",
          value: "18014398509481987.00",
          checked: null,
          read_only: true,
        },
      ],
    },
  ],
} as const;

describe("Simple View profile workflow", () => {
  it("decodes exact amounts and validated profile controls from the engine", () => {
    const result = {
      kind: "profile",
      payload: matched,
    };
    const parsed = decodeEngineReply(
      JSON.stringify({ ...replyScope, result }),
      "test",
    );
    expect(parsed.result).toEqual(result);
    const broken = {
      ...matched,
      groups: [
        {
          ...matched.groups[0],
          fields: [{ ...matched.groups[0].fields[0], checked: false }],
        },
      ],
    };
    expect(() =>
      decodeEngineReply(
        JSON.stringify({
          ...replyScope,
          result: { kind: "profile", payload: broken },
        }),
        "test",
      ),
    ).toThrow();
    expect(
      engineRequest(
        "test",
        {
          kind: "profile_preview",
          payload: {
            profile_id: "fixture-only",
            field_id: "score",
            input: { kind: "text", value: "9,007,199,254,740,993.50" },
          },
        },
        target,
      ).expected_revision,
    ).toBe("14");
  });

  it("renders exact controls only for a matched profile", () => {
    const base: BankEditorController = {
      document: {
        id: target.document_id,
        revision: target.expected_revision,
        name: "Training",
        xml: "<Bank/>",
        dirty: false,
        sections: [],
      },
    };
    const profile = {
      requireAuthor: true,
      onLoad: vi.fn(async () => ({ ok: true }) as const),
      onClear: vi.fn(async () => ({ ok: true }) as const),
      onPreview: vi.fn(async () => ({ preview_id: "1", changes: [] })),
      onApply: vi.fn(async () => ({ ok: true }) as const),
    };
    const ready = renderToStaticMarkup(
      <AppShell
        name="StarBank Web"
        editor={{ ...base, profile: { ...profile, current: matched } }}
      />,
    );
    expect(ready).toContain("Simple View");
    expect(ready).toContain("9007199254740993.50");
    expect(ready).toContain("Calculated bonus");
    expect(ready).toContain("Map author handle supplied by you");

    const outdated = renderToStaticMarkup(
      <AppShell
        name="StarBank Web"
        editor={{
          ...base,
          profile: { ...profile, current: { status: "incompatible" } },
        }}
      />,
    );
    expect(outdated).not.toContain("Simple View");
    expect(outdated).toContain("The profile requires a different bank layout");
    expect(outdated).toContain("Table View");
  });
});
