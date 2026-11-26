import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import {
  existsSync,
  mkdtempSync,
  readdirSync,
  readFileSync,
  realpathSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

import init, { WasmBankEngine } from "../src/generated/starbank_wasm.js";

const root = fileURLToPath(new URL("../../..", import.meta.url));
const wasm = new URL("../src/generated/starbank_wasm_bg.wasm", import.meta.url);
const cli = join(root, "target/debug/starbank");
const native = join(root, "target/debug/examples/parity_protocol");
const context = {
  author: "1-S2-1-1001",
  player: "2-S2-1-2002",
  bank_name: "AdapterParity",
};
const contextArgs = [
  "--author",
  context.author,
  "--player",
  context.player,
  "--bank-name",
  context.bank_name,
];
const editedInteger = "9007199254740993";
const groupedInteger = "9,007,199,254,740,993";

function sha256(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

// The fixed canonical preimage spells out stable sibling ordering, entity
// decoding, and ASCII replacement for each non-ASCII UTF-16 code unit.
const canonical = [
  context.author,
  context.player,
  context.bank_name,
  "?",
  "???",
  "Valueint",
  editedInteger,
  "Valuestring& ??,007",
].join("");
const signature = createHash("sha1")
  .update(canonical, "ascii")
  .digest("hex")
  .toUpperCase();

function withSignature(text) {
  return text.replace("</Bank>", `<Signature value="${signature}"/></Bank>`);
}

function sourceText(declaration) {
  return [
    `<?xml version="1.0" encoding="${declaration}"?>`,
    "<!-- snow 雪😀 -->",
    "<Bank version = '1'><Section name = '星'><Key name='值😀'>",
    '<Value int = "0007"/><!-- sibling -->',
    "<Value string='&amp; &#x1F642;,007'/></Key></Section></Bank>",
    "",
  ].join("\r\n");
}

function encode(text, encoding) {
  if (encoding === "utf-8") return Buffer.from(text, "utf8");
  const littleEndian = Buffer.from(`\uFEFF${text}`, "utf16le");
  return encoding === "utf-16le" ? littleEndian : littleEndian.swap16();
}

function run(binary, args, input, status = 0) {
  const process = spawnSync(binary, args, { input, encoding: "utf8" });
  if (process.error) throw process.error;
  assert.equal(
    process.status,
    status,
    [binary, ...args, process.stdout, process.stderr].join(" "),
  );
  return process.stdout;
}

function command(args, status = 0) {
  const report = JSON.parse(run(cli, [...args, "--json"], undefined, status));
  assert.equal(report.schema_version, 1);
  return report;
}

function checkProtocol(encoding, opening, changed, unsigned, signed) {
  const engine = new WasmBankEngine();
  const requests = [];
  const responses = [];
  let active;
  function dispatch(
    kind,
    expectedKind,
    { payload, attachment, revision } = {},
  ) {
    const id = String(requests.length + 1);
    const control = JSON.stringify({
      schema_version: 1,
      request_id: id,
      ...(kind === "open"
        ? {}
        : {
            document_id: active.document_id,
            expected_revision: revision ?? active.revision,
          }),
      command: { kind, payload },
    });
    requests.push({
      control,
      bytes: attachment ? Array.from(attachment) : null,
    });
    const reply = engine.dispatch(control, attachment);
    try {
      const result = JSON.parse(reply.take_control());
      const bytes = reply.take_bytes();
      assert.equal(result.schema_version, 1);
      assert.equal(result.request_id, id);
      assert.equal(result.result.kind, expectedKind, JSON.stringify(result));
      if (expectedKind === "snapshot") active = result;
      responses.push({
        control: result,
        bytes: bytes ? Array.from(bytes) : null,
      });
      return {
        payload: result.result.payload,
        bytes: bytes && Buffer.from(bytes),
      };
    } finally {
      reply.free();
    }
  }

  try {
    const original = encode(opening, encoding);
    const opened = dispatch("open", "snapshot", {
      payload: { context },
      attachment: original,
    });
    const value = opened.payload.sections[0].keys[0].values[0];
    assert.equal(value.value, "0007");
    assert.equal(
      opened.payload.sections[0].keys[0].values[1].value,
      "& 🙂,007",
    );
    assert.equal(active.revision, "0");
    assert.equal(opened.payload.dirty, false);
    const raw = dispatch("raw_text", "raw_text");
    assert.equal(raw.payload.source_encoding, encoding);
    assert.equal(
      raw.bytes.toString("utf8"),
      encoding === "utf-8" ? opening : `\uFEFF${opening}`,
    );
    assert.deepEqual(dispatch("raw", "raw").bytes, original);
    assert.deepEqual(dispatch("prepare", "prepared").bytes, original);

    const edits = { edits: [{ id: value.id, value: groupedInteger }] };
    const structured = dispatch("set_values", "snapshot", { payload: edits });
    assert.equal(
      structured.payload.sections[0].keys[0].values[0].value,
      editedInteger,
    );
    assert.equal(active.revision, "1");
    const stale = dispatch("set_values", "error", {
      payload: edits,
      revision: "0",
    });
    assert.equal(stale.payload.code, "stale_revision");
    assert.deepEqual(dispatch("prepare", "prepared").bytes, unsigned);
    const prepared = dispatch("prepare", "prepared", { payload: { context } });
    assert.equal(prepared.payload.signature_status, "valid");
    assert.deepEqual(prepared.bytes, signed);
    assert.equal(dispatch("snapshot", "snapshot").payload.dirty, true);
    assert.deepEqual(dispatch("opening_raw", "raw").bytes, original);
    assert.deepEqual(dispatch("raw", "raw").bytes, unsigned);

    dispatch("undo", "snapshot");
    assert.deepEqual(dispatch("raw", "raw").bytes, original);
    const otherEncoding = encoding === "utf-16le" ? "utf-16be" : "utf-16le";
    const otherText = changed.replace(/encoding="[^"]+"/, 'encoding="UTF-16"');
    const wrongEncoding = dispatch("replace_raw", "error", {
      attachment: encode(otherText, otherEncoding),
    });
    assert.equal(wrongEncoding.payload.code, "raw_encoding_mismatch");
    const malformed = dispatch("replace_raw_text", "error", {
      attachment: Buffer.from(changed.replace(editedInteger, groupedInteger)),
    });
    assert.equal(malformed.payload.code, "invalid_raw_number");
    const malformedXml = dispatch("replace_raw_text", "error", {
      attachment: Buffer.from(changed.replace("</Key>", "")),
    });
    assert.equal(malformedXml.payload.code, "invalid_document");
    assert.deepEqual(dispatch("raw", "raw").bytes, original);

    const rawEdit = dispatch("replace_raw_text", "snapshot", {
      attachment: Buffer.from(changed),
    });
    assert.equal(
      rawEdit.payload.sections[0].keys[0].values[0].value,
      editedInteger,
    );
    assert.deepEqual(dispatch("prepare", "prepared").bytes, unsigned);
    assert.deepEqual(
      dispatch("prepare", "prepared", { payload: { context } }).bytes,
      signed,
    );
    const extraText = changed.replace(
      "<!-- sibling -->",
      "<!-- changed 🚀 -->",
    );
    dispatch("replace_raw_text", "snapshot", {
      attachment: Buffer.from(extraText),
    });
    assert.deepEqual(dispatch("raw", "raw").bytes, encode(extraText, encoding));
    assert.deepEqual(
      dispatch("prepare", "prepared", { payload: { context } }).bytes,
      encode(withSignature(extraText), encoding),
    );
    const replay = requests
      .map((request) => JSON.stringify(request))
      .join("\n");
    const nativeResponses = run(native, [], `${replay}\n`)
      .trimEnd()
      .split("\n")
      .map((line) => JSON.parse(line));
    assert.deepEqual(nativeResponses, responses);
  } finally {
    engine.free();
  }
  return requests.length;
}

function checkCli(dir, original, unsigned, signed) {
  const source = join(dir, "source.sc2bank");
  const untouched = join(dir, "untouched.sc2bank");
  const unsignedCopy = join(dir, "unsigned.sc2bank");
  const signedCopy = join(dir, "signed.sc2bank");
  const patch = join(dir, "edit.json");
  const conflict = join(dir, "conflict.sc2bank");
  writeFileSync(source, original);
  const inspection = command(["inspect", source]).result;
  assert.equal(inspection.sha256, sha256(original));
  assert.equal(inspection.contents[0].keys[0].values[0].value, "0007");
  assert.equal(inspection.contents[0].keys[0].values[1].value, "& 🙂,007");
  const operation = {
    op: "set_value",
    section: 0,
    key: 0,
    value: 0,
    lexeme: groupedInteger,
  };
  const patchSpec = {
    schema_version: 1,
    expected_sha256: inspection.sha256,
    operations: [operation],
  };
  writeFileSync(patch, JSON.stringify(patchSpec));

  const exportReport = command([
    "export",
    source,
    "--output",
    untouched,
    "--no-resign",
  ]);
  assert.equal(exportReport.result.diff.source_changed, false);
  assert.deepEqual(readFileSync(untouched), original);
  const beforeDryRun = readdirSync(dir).sort();
  const edit = ["edit", source, "--patch", patch, "--output"];
  const dryUnsigned = command([
    ...edit,
    unsignedCopy,
    "--no-resign",
    "--dry-run",
  ]).result;
  assert.equal(dryUnsigned.outcome, "prepared");
  assert.equal(dryUnsigned.diff.before_sha256, sha256(original));
  assert.equal(dryUnsigned.diff.after_sha256, sha256(unsigned));
  const drySigned = command([
    ...edit,
    signedCopy,
    "--dry-run",
    ...contextArgs,
  ]).result;
  assert.equal(drySigned.outcome, "prepared");
  assert.equal(drySigned.signature, "valid");
  assert.equal(drySigned.diff.stored_signature.after, signature);
  assert.equal(drySigned.diff.after_sha256, sha256(signed));
  const dryInPlace = command([
    "edit",
    source,
    "--patch",
    patch,
    "--in-place",
    "--dry-run",
    ...contextArgs,
  ]).result;
  assert.equal(dryInPlace.outcome, "prepared");
  assert.equal(dryInPlace.diff.after_sha256, sha256(signed));
  assert.deepEqual(readdirSync(dir).sort(), beforeDryRun);
  assert.deepEqual(readFileSync(source), original);

  command([...edit, unsignedCopy, "--no-resign"]);
  const written = command([...edit, signedCopy, ...contextArgs]).result;
  assert.equal(written.diff.after_sha256, drySigned.diff.after_sha256);
  assert.deepEqual(readFileSync(unsignedCopy), unsigned);
  assert.deepEqual(readFileSync(signedCopy), signed);
  const verified = command([
    "validate",
    signedCopy,
    "--require-signature",
    ...contextArgs,
  ]);
  assert.equal(verified.result.signature.status, "valid");
  assert.equal(verified.result.signature.digest, signature);
  assert.equal(verified.result.sha256, sha256(signed));
  assert.equal(
    command([...edit, signedCopy, ...contextArgs], 5).error.code,
    "destination_exists",
  );
  assert.deepEqual(readFileSync(signedCopy), signed);
  writeFileSync(
    patch,
    JSON.stringify({ ...patchSpec, expected_sha256: "f".repeat(64) }),
  );
  assert.equal(command([...edit, conflict], 5).error.code, "input_changed");
  assert.equal(existsSync(conflict), false);
  assert.deepEqual(readFileSync(source), original);
}

await init({ module_or_path: readFileSync(wasm) });
for (const [encoding, declared] of [
  ["utf-8", "UTF-8"],
  ["utf-16le", "UTF-16"],
  ["utf-16be", "UTF-16BE"],
]) {
  const opening = sourceText(declared);
  const changed = opening.replace("0007", editedInteger);
  const original = encode(opening, encoding);
  const unsigned = encode(changed, encoding);
  const signed = encode(withSignature(changed), encoding);
  const directory = mkdtempSync(
    join(realpathSync(tmpdir()), "starbank-parity-"),
  );
  try {
    const requests = checkProtocol(
      encoding,
      opening,
      changed,
      unsigned,
      signed,
    );
    checkCli(directory, original, unsigned, signed);
    console.log(
      JSON.stringify({
        encoding,
        declared,
        requests,
        source_sha256: sha256(original),
        edited_sha256: sha256(unsigned),
        signature,
        signed_sha256: sha256(signed),
      }),
    );
  } finally {
    rmSync(directory, { recursive: true });
  }
}
