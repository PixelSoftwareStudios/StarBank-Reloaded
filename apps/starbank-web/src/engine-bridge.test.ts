import {
  prepareBankDownload,
  prepareOpeningBankBackup,
  readBankFile,
} from "starbank-ui/browser";
import { EngineWorkerClient } from "starbank-ui/engine";
import { decodeEngineReply, engineRequest } from "starbank-ui/engine/protocol";
import { describe, expect, test, vi } from "vitest";

const BANK_LIMIT = 16;
const REQUEST_LIMIT = 32;
const ready = {
  kind: "ready",
  version: 1,
  info: {
    schemaVersion: 1,
    maxBankBytes: BANK_LIMIT,
    maxControlBytes: REQUEST_LIMIT,
  },
};

class TestWorker extends EventTarget {
  readonly requests: unknown[] = [];
  terminated = false;

  postMessage(
    message: unknown,
    options?: Transferable[] | StructuredSerializeOptions,
  ): void {
    const cloneOptions = Array.isArray(options)
      ? { transfer: options }
      : options;
    this.requests.push(structuredClone(message, cloneOptions));
  }

  terminate(): void {
    this.terminated = true;
  }

  receive(data: unknown): void {
    this.dispatchEvent(new MessageEvent("message", { data }));
  }
}

describe("worker transport", () => {
  test("allows a single pending request and transfers only the selected buffer", async () => {
    const worker = new TestWorker();
    const client = new EngineWorkerClient(worker);
    const input = new Uint8Array([0x11, 0x22]).buffer;
    const first = client.request("open", input);
    await expect(client.request("another open")).rejects.toMatchObject({
      code: "busy",
    });

    worker.receive(ready);
    await vi.waitFor(() => expect(worker.requests).toHaveLength(1));
    expect(input.byteLength).toBe(0);
    expect(worker.requests[0]).toEqual({
      id: 1,
      control: "open",
      bytes: new Uint8Array([0x11, 0x22]).buffer,
    });
    await expect(client.request("another open")).rejects.toMatchObject({
      code: "busy",
    });

    worker.receive({ kind: "response", id: 1, control: "opened" });
    await expect(first).resolves.toMatchObject({ control: "opened" });
    client.close();
  });

  test("ignores an obsolete reply while completing the matching command", async () => {
    const worker = new TestWorker();
    const client = new EngineWorkerClient(worker);
    worker.receive(ready);

    const first = client.request("first");
    await vi.waitFor(() => expect(worker.requests).toHaveLength(1));
    worker.receive({ kind: "response", id: 1, control: "first reply" });
    await first;

    const second = client.request("second");
    await vi.waitFor(() => expect(worker.requests).toHaveLength(2));
    worker.receive({ kind: "response", id: 1, control: "old reply" });
    worker.receive({ kind: "response", id: 2, control: "new reply" });
    await expect(second).resolves.toMatchObject({ control: "new reply" });
    client.close();
  });

  test("rejects oversized bytes and control before sending either", async () => {
    const worker = new TestWorker();
    const client = new EngineWorkerClient(worker);
    worker.receive(ready);
    const bytes = new ArrayBuffer(BANK_LIMIT + 1);
    await expect(client.request("open", bytes)).rejects.toMatchObject({
      code: "limit",
    });
    await expect(
      client.request("x".repeat(REQUEST_LIMIT + 1)),
    ).rejects.toMatchObject({ code: "limit" });
    await expect(
      client.request("×".repeat(REQUEST_LIMIT)),
    ).rejects.toMatchObject({
      code: "limit",
    });
    expect(bytes.byteLength).toBe(BANK_LIMIT + 1);
    expect(worker.requests).toHaveLength(0);
    client.close();
  });

  test("ends the uncertain session on a reply for an unissued request", async () => {
    const worker = new TestWorker();
    const client = new EngineWorkerClient(worker);
    worker.receive(ready);
    const active = client.request("edit");
    await vi.waitFor(() => expect(worker.requests).toHaveLength(1));

    worker.receive({ kind: "response", id: 2, control: "future reply" });
    await expect(active).rejects.toMatchObject({ code: "protocol" });
    await expect(client.request("again")).rejects.toMatchObject({
      code: "protocol",
    });
    expect(worker.terminated).toBe(true);
  });

  test("rejects a handshake for an unsupported editor protocol", async () => {
    const worker = new TestWorker();
    const client = new EngineWorkerClient(worker);
    worker.receive({ ...ready, info: { ...ready.info, schemaVersion: 2 } });
    await expect(client.ready()).rejects.toMatchObject({ code: "protocol" });
    expect(worker.terminated).toBe(true);
  });
});

describe("local file adapter and protocol", () => {
  test("checks file size before reading and preserves binary data on download", async () => {
    const file = new File(
      [new Uint8Array([0xef, 0xbb, 0xbf, 0x0a])],
      "payload.SC2Bank",
    );
    const read = vi.spyOn(file, "arrayBuffer");
    await expect(readBankFile(file, file.size - 1)).rejects.toThrow(RangeError);
    expect(read).not.toHaveBeenCalled();

    const result = await readBankFile(file, file.size);
    const download = prepareBankDownload(result.bytes, result.name);
    try {
      expect(download.name).toBe(file.name);
      expect(
        new Uint8Array(await (await fetch(download.href)).arrayBuffer()),
      ).toEqual(new Uint8Array([0xef, 0xbb, 0xbf, 0x0a]));
    } finally {
      download.release();
    }
  });

  test("packages an opening backup without decoding source bytes", async () => {
    const source = Uint8Array.from([0xfe, 0xff, 0x00, 0x3c, 0x00, 0x42]);
    const backup = prepareOpeningBankBackup(
      source.buffer,
      "Training.SC2BANK",
      new Date("2026-11-26T22:31:05Z"),
    );
    try {
      expect(backup.name).toBe("Training_20261126_223105.sc2bank.bak");
      const downloaded = await (await fetch(backup.href)).arrayBuffer();
      expect(new Uint8Array(downloaded)).toEqual(source);
    } finally {
      backup.release();
    }
  });

  test("decodes validation issues and rejects malformed issue records", () => {
    const scope = {
      schema_version: 1,
      request_id: "check",
      document_id: "d1",
      revision: "2",
    };
    const issue = {
      id: "v1",
      code: "invalid_integer",
      message: "Enter an integer with optional comma grouping.",
    };
    const reply = (issues: unknown[]) =>
      JSON.stringify({
        ...scope,
        result: { kind: "value_validation", payload: { issues } },
      });
    expect(decodeEngineReply(reply([issue]), "check").result).toEqual({
      kind: "value_validation",
      payload: { issues: [issue] },
    });
    expect(() =>
      decodeEngineReply(reply([{ ...issue, code: "unknown" }]), "check"),
    ).toThrow("invalid reply");
    expect(() =>
      decodeEngineReply(reply([{ ...issue, id: undefined }]), "check"),
    ).toThrow("invalid reply");
  });

  test("preserves maximum revision text and rejects an uncorrelated response", () => {
    const revision = "18446744073709551615";
    const request = engineRequest(
      "xml",
      { kind: "raw" },
      { document_id: "d1", expected_revision: revision },
    );
    expect(request.expected_revision).toBe(revision);

    const reply = JSON.stringify({
      schema_version: 1,
      request_id: "xml",
      document_id: "d1",
      revision,
      result: { kind: "raw", payload: {} },
    });
    expect(decodeEngineReply(reply, "xml").revision).toBe(revision);
    expect(() => decodeEngineReply(reply, "other")).toThrow("invalid reply");
    expect(() =>
      decodeEngineReply(reply.replace(revision, "18446744073709551616"), "xml"),
    ).toThrow("invalid reply");
  });
});
