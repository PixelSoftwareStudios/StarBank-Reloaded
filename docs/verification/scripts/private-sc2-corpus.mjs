import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import {
  closeSync,
  fstatSync,
  openSync,
  readSync,
  realpathSync,
  statSync,
} from "node:fs";
import { dirname, isAbsolute, relative, resolve, sep } from "node:path";
import { pathToFileURL } from "node:url";
import { isDeepStrictEqual } from "node:util";

const manifestLimit = 1024 * 1024;
const documentLimit = 8 * 1024 * 1024;
const archiveLimit = 512 * 1024 * 1024;
const maxCases = 512;
const sha256Pattern = /^[0-9a-f]{64}$/;
const origins = new Set(["synthetic", "owner_file", "game"]);
const encodings = new Set(["utf-8", "utf-16le", "utf-16be"]);
const signatures = new Set([
  "unsigned",
  "malformed",
  "missing_context",
  "unsupported",
  "valid",
  "invalid",
]);

class CorpusFailure extends Error {}

function requireCheck(condition, reason) {
  if (!condition) throw new CorpusFailure(reason);
}

function text(value) {
  return typeof value === "string" && value.trim().length > 0;
}

function regularFile(path) {
  requireCheck(statSync(path).isFile(), "expected a regular local file");
  return path;
}

function capture(path, limit, digestOnly = false) {
  regularFile(path);
  const fd = openSync(path, "r");
  try {
    const size = fstatSync(fd).size;
    requireCheck(size <= limit, "input exceeds the verifier limit");
    const bytes = Buffer.alloc(digestOnly ? Math.min(size, 64 * 1024) : size);
    const digest = createHash("sha256");
    for (let offset = 0; offset < size; ) {
      const start = digestOnly ? 0 : offset;
      const count = readSync(
        fd,
        bytes,
        start,
        Math.min(size - offset, bytes.length - start),
        offset,
      );
      requireCheck(count > 0, "input changed while being read");
      digest.update(bytes.subarray(start, start + count));
      offset += count;
    }
    requireCheck(
      readSync(fd, Buffer.alloc(1), 0, 1, size) === 0,
      "input grew while being read",
    );
    return { sha256: digest.digest("hex"), size, bytes };
  } finally {
    closeSync(fd);
  }
}

function selectedPath(folder, name) {
  requireCheck(
    text(name) && !isAbsolute(name),
    "fixture path must be relative to the manifest",
  );
  const path = resolve(folder, name);
  const scoped = relative(folder, path);
  requireCheck(
    scoped && scoped !== ".." && !scoped.startsWith(`..${sep}`),
    "fixture path leaves the manifest directory",
  );
  requireCheck(realpathSync(path) === path, "fixture path resolves elsewhere");
  return path;
}

function verifyRecord(entry) {
  const source = entry?.source;
  requireCheck(
    ["bank", "map"].includes(entry?.kind) &&
      Number.isSafeInteger(entry.byteLength) &&
      entry.byteLength >= 0 &&
      typeof entry.sha256 === "string" &&
      sha256Pattern.test(entry.sha256),
    "invalid fixture record, length or checksum",
  );
  requireCheck(
    origins.has(source?.origin) &&
      text(source.capture) &&
      text(source.authorization) &&
      text(source.expectationSource),
    "missing capture, authorization or independent expectation source",
  );
  if (source.origin === "game") {
    requireCheck(
      text(source.gameBuild) &&
        text(source.platform) &&
        text(source.mapRevision),
      "game fixture needs a build, platform and map revision",
    );
  }
}

function run(
  binary,
  args,
  { input, timeout = 30_000, maxBuffer = manifestLimit } = {},
) {
  const result = spawnSync(binary, args, { input, timeout, maxBuffer });
  requireCheck(
    !result.error && result.signal === null,
    "adapter did not complete",
  );
  // Stdout can contain data, paths and signing identities. Never relay it or stderr.
  return result;
}

function request(kind, sequence, payload, active) {
  return {
    schema_version: 1,
    request_id: String(sequence),
    ...(active && {
      document_id: active.document_id,
      expected_revision: active.revision,
    }),
    command: { kind, payload },
  };
}

function resultOf(reply, requestId, kind) {
  requireCheck(
    reply?.schema_version === 1 &&
      reply.request_id === String(requestId) &&
      reply.result?.kind === kind,
    "bank protocol returned an unexpected reply",
  );
  return reply.result.payload;
}

function bankProjection(sections) {
  return sections.map(({ name, keys }) => [
    name,
    keys.map((key) => [
      key.name,
      key.values.map((value) => [value.name, value.kind, value.value]),
    ]),
  ]);
}

function verifyBank(engineClass, native, bytes, entry) {
  const { context, expected } = entry;
  requireCheck(
    context === null ||
      (text(context?.author) &&
        text(context.player) &&
        text(context.bank_name)),
    "bank needs a complete signing context or explicit null",
  );
  const error = expected?.openError;
  if (error !== undefined) {
    requireCheck(error === "invalid_document", "unsupported bank expectation");
  } else {
    requireCheck(
      encodings.has(expected?.encoding) &&
        signatures.has(expected.signatureStatus) &&
        (expected.signatureStatus !== "valid" || context !== null) &&
        Array.isArray(expected.projection),
      "bank needs independent encoding, signature and ordered projection expectations",
    );
  }

  const engine = new engineClass();
  const transcript = [];
  function dispatch(kind, payload, active, attachment) {
    const control = JSON.stringify(
      request(kind, transcript.length + 1, payload, active),
    );
    const response = engine.dispatch(control, attachment);
    try {
      const reply = {
        control: JSON.parse(response.take_control()),
        bytes: response.take_bytes() ?? null,
      };
      transcript.push({ control, attachment, reply });
      return reply;
    } finally {
      response.free();
    }
  }

  try {
    const opened = dispatch(
      "open",
      context === null ? undefined : { context },
      undefined,
      bytes,
    );
    if (error !== undefined) {
      requireCheck(
        resultOf(opened.control, 1, "error").code === error && !opened.bytes,
        "bank refusal differed from the independent expectation",
      );
    } else {
      const projection = resultOf(opened.control, 1, "snapshot");
      requireCheck(
        projection.signature_status === expected.signatureStatus,
        "bank signature state differed from the independent expectation",
      );
      requireCheck(
        isDeepStrictEqual(
          bankProjection(projection.sections),
          expected.projection,
        ),
        "bank ordered projection differed from the independent expectation",
      );
      const decoded = dispatch("raw_text", undefined, opened.control);
      requireCheck(
        resultOf(decoded.control, 2, "raw_text").source_encoding ===
          expected.encoding,
        "bank encoding differed from the independent expectation",
      );
      const exported = dispatch("prepare", undefined, opened.control);
      requireCheck(
        resultOf(exported.control, 3, "prepared").signature_status ===
          expected.signatureStatus &&
          exported.bytes &&
          Buffer.from(exported.bytes).equals(bytes),
        "bank export did not preserve its signature state and exact bytes",
      );
      const reopened = dispatch(
        "open",
        context === null ? undefined : { context },
        undefined,
        exported.bytes,
      );
      requireCheck(
        isDeepStrictEqual(
          resultOf(reopened.control, 4, "snapshot"),
          projection,
        ),
        "exported bank did not reparse to the same projection",
      );
      if (expected.signedSha256 !== undefined) {
        requireCheck(
          context &&
            typeof expected.signedSha256 === "string" &&
            sha256Pattern.test(expected.signedSha256),
          "signed output requires explicit context and an independent checksum",
        );
        const signed = dispatch("prepare", { context }, reopened.control);
        requireCheck(
          resultOf(signed.control, 5, "prepared").signature_status ===
            "valid" &&
            signed.bytes &&
            createHash("sha256").update(signed.bytes).digest("hex") ===
              expected.signedSha256,
          "signed bank differed from the independent output checksum",
        );
        const reparsed = dispatch("open", { context }, undefined, signed.bytes);
        requireCheck(
          resultOf(reparsed.control, 6, "snapshot").signature_status ===
            "valid",
          "signed bank could not be reparsed and verified",
        );
      }
    }
    verifyNative(native, transcript);
  } finally {
    engine.free();
  }
}

function verifyNative(native, transcript) {
  const input = transcript
    .map(({ control, attachment }) =>
      JSON.stringify({
        control,
        bytes: attachment ? Array.from(attachment) : null,
      }),
    )
    .join("\n");
  const result = run(native, [], {
    input,
    maxBuffer: 16 * documentLimit,
  });
  requireCheck(result.status === 0, "native protocol exited unsuccessfully");
  const replies = result.stdout.toString("utf8").trimEnd().split("\n");
  requireCheck(
    replies.length === transcript.length,
    "native reply count differed",
  );
  for (const [index, { reply }] of transcript.entries()) {
    const nativeReply = JSON.parse(replies[index]);
    requireCheck(
      isDeepStrictEqual(nativeReply.control, reply.control) &&
        (reply.bytes
          ? Array.isArray(nativeReply.bytes) &&
            Buffer.from(nativeReply.bytes).equals(Buffer.from(reply.bytes))
          : nativeReply.bytes === null),
      "native and WASM bank replies differed",
    );
  }
}

function verifyMap(binary, path, expected) {
  requireCheck(
    Number.isInteger(expected?.exitStatus) &&
      expected.exitStatus >= 0 &&
      expected.exitStatus <= 7 &&
      Array.isArray(expected.issueCodes) &&
      expected.issueCodes.every(text),
    "map needs independently declared status and diagnostics",
  );
  const result = run(binary, ["map", "scan", "--root", path, "--json"]);
  requireCheck(
    result.status === expected.exitStatus,
    "map exit status differed from the independent expectation",
  );
  const reply = JSON.parse(result.stdout.toString("utf8"));
  requireCheck(
    reply.schema_version === 1 && reply.command === "map_scan",
    "unexpected CLI map report",
  );
  const { issues, maps, incomplete } = reply.result;
  requireCheck(
    isDeepStrictEqual(
      issues.map((issue) => issue.error.code),
      expected.issueCodes,
    ) && incomplete === expected.issueCodes.length > 0,
    "map diagnostics differed from the independent expectation",
  );
  requireCheck(
    maps.length === (expected.metadata === null ? 0 : 1),
    "map record count differed from the independent expectation",
  );
  if (maps.length === 0) return;
  const [record] = maps;
  requireCheck(
    isDeepStrictEqual(
      {
        title: record.title,
        titleSource: record.title_source,
        author: record.author,
        bankNames: record.bank_names,
        hasScript: record.has_script,
        declaredBlizzard: record.declared_blizzard,
      },
      expected.metadata,
    ),
    "map metadata differed from the independent expectation",
  );
}

function reportFailure(position, error) {
  const reason =
    error instanceof CorpusFailure
      ? error.message
      : "local input or adapter output could not be read";
  process.stderr.write(`${position}: ${reason}\n`);
}

async function main(args) {
  requireCheck(
    args.length === 5,
    "usage: private-sc2-corpus.mjs MANIFEST NATIVE_PROTOCOL CLI WASM_JS WASM_BINARY",
  );
  const [manifestArg, nativeArg, cliArg, wasmJs, wasmBinary] = args;
  const manifestPath = realpathSync(manifestArg);
  const folder = dirname(manifestPath);
  const manifest = JSON.parse(capture(manifestPath, manifestLimit).bytes);
  requireCheck(
    manifest.schemaVersion === 1 &&
      Array.isArray(manifest.cases) &&
      manifest.cases.length > 0 &&
      manifest.cases.length <= maxCases,
    "invalid corpus manifest version or case count",
  );
  const native = regularFile(resolve(nativeArg));
  const cli = regularFile(resolve(cliArg));
  const { default: init, WasmBankEngine } = await import(
    pathToFileURL(resolve(wasmJs)).href
  );
  await init({
    module_or_path: capture(resolve(wasmBinary), documentLimit).bytes,
  });
  const passed = Object.fromEntries(
    Array.from(origins, (origin) => [origin, { bank: 0, map: 0 }]),
  );
  let failures = 0;
  for (const [index, entry] of manifest.cases.entries()) {
    try {
      verifyRecord(entry);
      const path = selectedPath(folder, entry.path);
      const bank = entry.kind === "bank";
      const limit = bank ? documentLimit : archiveLimit;
      const loaded = capture(path, limit, !bank);
      requireCheck(
        loaded.size === entry.byteLength && loaded.sha256 === entry.sha256,
        "fixture length or SHA-256 differed from provenance",
      );
      if (bank) verifyBank(WasmBankEngine, native, loaded.bytes, entry);
      else verifyMap(cli, path, entry.expected);
      requireCheck(
        capture(path, limit, true).sha256 === entry.sha256,
        "source bytes changed during verification",
      );
      passed[entry.source.origin][entry.kind] += 1;
    } catch (error) {
      failures += 1;
      reportFailure(`Case ${index + 1}`, error);
    }
  }
  process.stdout.write(
    `Passed ${passed.synthetic.bank} synthetic banks, ${passed.synthetic.map} synthetic maps, ${passed.owner_file.bank} owner-supplied banks, ${passed.owner_file.map} owner-supplied maps, ${passed.game.bank} game banks and ${passed.game.map} game maps. Failed ${failures}.\n`,
  );
  if (failures) process.exitCode = 1;
}

try {
  await main(process.argv.slice(2));
} catch (error) {
  reportFailure("Corpus", error);
  process.exitCode = 1;
}
