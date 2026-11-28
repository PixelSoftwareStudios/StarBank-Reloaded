import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const bankName = "legacy-fixture.SC2Bank";
const contextRequired = "profile_context_required";
const authorHandle = "9-S2-1-123";
const legacyDefinition = new URL(
  "../../../crates/domain/starbank-profiles/tests/fixtures/legacy.yaml",
  import.meta.url,
);

export function createDesktopProfileImportFixture() {
  let wrongScope;
  return {
    name: bankName,
    bank: Buffer.from(
      '<Bank><Section name="Counts"><Key name="coins"><Value int="0001"/></Key><Key name="hints"><Value int="0"/></Key><Key name="display"><Value string="Synthetic"/></Key></Section></Bank>',
    ),
    changeNextReplyScope(field, value, message) {
      assert.equal(wrongScope, undefined);
      wrongScope = { field, value, message };
    },
    transformReply(request, reply) {
      if (!wrongScope || request.command.kind !== "profile_load") return reply;
      const { field, value, message } = wrongScope;
      wrongScope = undefined;
      const control = JSON.parse(reply.control);
      assert.equal(control.result.kind, "error");
      assert.equal(control.result.payload.code, contextRequired);
      control[field] = value;
      control.result.payload.message = message;
      return { ...reply, control: JSON.stringify(control) };
    },
  };
}

function assertUnchangedBank(engine, target, bytes) {
  const reply = engine.dispatch(
    JSON.stringify({
      schema_version: 1,
      request_id: "legacy-profile-verification",
      document_id: target.document_id,
      expected_revision: target.expected_revision,
      command: { kind: "raw" },
    }),
  );
  try {
    const control = JSON.parse(reply.take_control());
    assert.equal(control.result.kind, "raw");
    assert.equal(control.revision, target.expected_revision);
    assert.deepEqual(Buffer.from(reply.take_bytes()), bytes);
  } finally {
    reply.free();
  }
}

export async function verifyDesktopProfileImport(
  page,
  engine,
  bridge,
  fixture,
) {
  const button = (name) => page.getByRole("button", { name, exact: true });
  await button("Open Bank…").click();
  await page
    .getByRole("heading", { name: fixture.name, exact: true })
    .waitFor();
  const active = bridge.wireRequests.findLast(
    ({ command }) => command.kind === "raw_text",
  );
  assert(active);

  const openProfilePicker = () =>
    page.getByText("Bank profile", { exact: true }).click();
  await openProfilePicker();
  const author = page.getByLabel("Map author handle supplied by you");
  const simple = page.getByRole("tab", { name: "Simple View", exact: true });
  const fileInput = page.getByLabel("Bank profile file", { exact: true });
  const status = page.locator(".editor-status");
  const yaml = await readFile(legacyDefinition);
  const load = () =>
    fileInput.setInputFiles({
      name: "legacy.yaml",
      mimeType: "application/yaml",
      buffer: yaml,
    });

  for (const [field, value, message] of [
    [
      "document_id",
      "different-document",
      "Profile reply used a different document.",
    ],
    ["revision", "9", "Profile reply used a different revision."],
  ]) {
    fixture.changeNextReplyScope(field, value, message);
    await load();
    await status.filter({ hasText: message }).waitFor();
    assert.equal(await author.count(), 0);
    assert.equal(await simple.count(), 0);
  }

  await load();
  await author.waitFor();
  assert.equal(await simple.count(), 0);
  assertUnchangedBank(engine, active, fixture.bank);
  await author.fill(authorHandle);
  await load();
  await simple.waitFor();
  const coins = page.getByRole("textbox", { name: "Coins", exact: true });
  await coins.waitFor();
  assert.equal(await coins.inputValue(), "0001");
  assert.equal(await author.count(), 0);
  assertUnchangedBank(engine, active, fixture.bank);
  const attempts = bridge.wireRequests.filter(
    ({ command }) => command.kind === "profile_load",
  );
  assert.equal(attempts.length, 4);
  for (const attempt of attempts) {
    assert.equal(attempt.document_id, active.document_id);
    assert.equal(attempt.expected_revision, active.expected_revision);
  }
  assert.deepEqual(attempts[3].command.payload, {
    format: "legacy_yaml",
    profile_id: "legacy",
    identity: { author: authorHandle, bank_name: "legacy-fixture" },
  });

  await button("Clear profile").click();
  await simple.waitFor({ state: "detached" });
  await load();
  await author.waitFor();
  await button("Open Bank…").click();
  await page
    .getByRole("heading", { name: "Replacement-2.SC2Bank", exact: true })
    .waitFor();
  await openProfilePicker();
  assert.equal(await author.count(), 0);
}
