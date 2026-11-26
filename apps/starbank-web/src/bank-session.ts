import { type EngineInfo, EngineWorkerClient } from "@starbank/engine-client";
import {
  decodeEngineReply,
  type EngineCommand,
  type EngineDiff,
  type EngineProfile,
  type EngineProfileEdit,
  type EngineProfilePreview,
  type EngineSignatureStatus,
  type EngineSigningContext,
  type EngineSnapshot,
  type EngineTarget,
  engineRequest,
  profileLoadForFile,
} from "@starbank/engine-client/protocol";
import { readBankFile } from "@starbank/platform/browser";
import type { BankEditorDocument, EditorTarget } from "@starbank/ui";

const xmlEncoder = new TextEncoder();
const xmlDecoder = new TextDecoder("utf-8", { fatal: true, ignoreBOM: true });

const signatureText: Record<EngineSignatureStatus, string> = {
  unsigned: "Unsigned",
  malformed: "Malformed signature",
  missing_context: "Signing context required to verify",
  unsupported: "Signature verification unsupported",
  valid: "Signature verified",
  invalid: "Invalid signature",
};

export type BankEditCommand = Extract<
  EngineCommand,
  {
    kind:
      | "set_values"
      | "undo"
      | "redo"
      | "reset"
      | "structure"
      | "revert"
      | "profile_apply";
  }
>;

type ProfileCommand = Extract<
  EngineCommand,
  { kind: "profile_load" | "profile_snapshot" | "profile_clear" }
>;

interface PreparedBank {
  readonly bytes: ArrayBuffer;
  readonly signatureStatus: EngineSignatureStatus;
}

export function createBankSession(): BrowserBankSession {
  return new BrowserBankSession(
    new EngineWorkerClient(
      new Worker(new URL("./engine-worker.ts", import.meta.url), {
        type: "module",
      }),
    ),
  );
}

export class BrowserBankSession {
  #sequence = 0;

  constructor(private readonly client: EngineWorkerClient) {}

  ready(): Promise<EngineInfo> {
    return this.client.ready();
  }

  close(): void {
    this.client.close();
  }

  async open(file: File, discardCurrent = false): Promise<BankEditorDocument> {
    const { maxBankBytes } = await this.client.ready();
    const { bytes, name } = await readBankFile(file, maxBankBytes).catch(
      (error: unknown) => {
        if (error instanceof RangeError) throw error;
        throw new Error(
          "The bank file could not be read. Select it again and retry.",
        );
      },
    );
    const command: EngineCommand = {
      kind: "open",
      payload: discardCurrent ? { discard_current: true } : undefined,
    };
    return this.#update(name, command, undefined, bytes);
  }

  async edit(
    name: string,
    target: EditorTarget,
    command: BankEditCommand,
  ): Promise<BankEditorDocument> {
    return this.#update(name, command, this.#target(target));
  }

  async replaceXml(
    name: string,
    target: EditorTarget,
    xml: string,
  ): Promise<BankEditorDocument> {
    const { maxBankBytes } = await this.client.ready();
    if (xml.length > maxBankBytes) {
      throw new RangeError("This bank exceeds the engine's file size limit.");
    }
    return this.#update(
      name,
      { kind: "replace_raw" },
      this.#target(target),
      encodeBankXml(xml),
    );
  }

  async diff(target: EditorTarget): Promise<EngineDiff> {
    const { reply } = await this.#dispatch(
      { kind: "diff" },
      this.#target(target),
    );
    if (reply.result.kind !== "diff" || reply.revision !== target.revision) {
      throw new Error(
        "The bank engine did not return changes for the requested revision.",
      );
    }
    return reply.result.payload;
  }

  async loadProfile(
    target: EditorTarget,
    bankName: string,
    file: File,
    author?: string,
  ): Promise<EngineProfile> {
    const payload = profileLoadForFile(file, bankName, author);
    const bytes = await file.arrayBuffer();
    return this.#profile(target, { kind: "profile_load", payload }, bytes);
  }

  profile(target: EditorTarget): Promise<EngineProfile> {
    return this.#profile(target, { kind: "profile_snapshot" });
  }

  clearProfile(target: EditorTarget): Promise<EngineProfile> {
    return this.#profile(target, { kind: "profile_clear" });
  }

  async previewProfile(
    target: EditorTarget,
    payload: EngineProfileEdit,
  ): Promise<EngineProfilePreview> {
    const { reply } = await this.#dispatch(
      { kind: "profile_preview", payload },
      this.#target(target),
    );
    if (
      reply.result.kind !== "profile_preview" ||
      reply.revision !== target.revision
    ) {
      throw new Error(
        "The bank engine did not return a profile preview for this revision.",
      );
    }
    return reply.result.payload;
  }

  async #profile(
    target: EditorTarget,
    command: ProfileCommand,
    bytes?: ArrayBuffer,
  ): Promise<EngineProfile> {
    const { reply } = await this.#dispatch(
      command,
      this.#target(target),
      bytes,
    );
    if (reply.result.kind !== "profile" || reply.revision !== target.revision) {
      throw new Error(
        "The bank engine did not return a profile for this revision.",
      );
    }
    return reply.result.payload;
  }

  async prepare(
    target: EditorTarget,
    context?: EngineSigningContext,
  ): Promise<PreparedBank> {
    const { reply, bytes } = await this.#dispatch(
      { kind: "prepare", payload: context ? { context } : undefined },
      this.#target(target),
    );
    if (
      reply.result.kind !== "prepared" ||
      bytes === undefined ||
      reply.revision !== target.revision
    ) {
      throw new Error(
        "The bank engine did not return a prepared export for this revision.",
      );
    }
    return { bytes, signatureStatus: reply.result.payload.signature_status };
  }

  #target(target: EditorTarget): EngineTarget {
    return { document_id: target.id, expected_revision: target.revision };
  }

  async #update(
    name: string,
    command: EngineCommand,
    target?: EngineTarget,
    attachment?: ArrayBuffer,
  ) {
    const { reply } = await this.#dispatch(command, target, attachment);
    if (
      reply.result.kind !== "snapshot" ||
      reply.document_id === null ||
      reply.revision === null
    ) {
      throw new Error("The bank engine did not return a document.");
    }
    const { document_id, revision } = reply;
    const raw = await this.#dispatch(
      { kind: "raw" },
      { document_id, expected_revision: revision },
    );
    if (
      raw.reply.result.kind !== "raw" ||
      raw.bytes === undefined ||
      raw.reply.revision !== revision
    ) {
      throw new Error(
        "The bank engine did not return XML for the current document.",
      );
    }
    return projectDocument(
      name,
      document_id,
      revision,
      reply.result.payload,
      raw.bytes,
    );
  }

  async #dispatch(
    command: EngineCommand,
    target?: EngineTarget,
    bytes?: ArrayBuffer,
  ) {
    this.#sequence += 1;
    const requestId = String(this.#sequence);
    const request = engineRequest(requestId, command, target);
    const response = await this.client.request(JSON.stringify(request), bytes);
    const reply = decodeEngineReply(response.control, requestId);
    if (reply.result.kind === "error") {
      throw new Error(reply.result.payload.message);
    }
    if (target && target.document_id !== reply.document_id) {
      throw new Error("The bank engine returned a different document.");
    }
    return { reply, bytes: response.bytes };
  }
}

export function encodeBankXml(xml: string): ArrayBuffer {
  return xmlEncoder.encode(xml).buffer;
}

export function readPreparedXml(bytes: ArrayBuffer): string {
  return xmlDecoder.decode(bytes);
}

function projectDocument(
  name: string,
  id: string,
  revision: string,
  snapshot: EngineSnapshot,
  bytes: ArrayBuffer,
): BankEditorDocument {
  return {
    name,
    id,
    revision,
    xml: readPreparedXml(bytes),
    dirty: snapshot.dirty,
    signatureStatus: signatureText[snapshot.signature_status],
    sections: snapshot.sections.map((section) => ({
      id: section.id,
      name: section.name ?? "(Unnamed section)",
      keys: section.keys.map((key) => ({
        id: key.id,
        name: key.name ?? "(Unnamed key)",
      })),
      rows: section.keys.flatMap((key) =>
        key.values.map((value) => ({
          id: value.id,
          keyId: key.id,
          name: key.name ?? "(Unnamed key)",
          childName: value.name,
          type: value.kind ?? "Opaque",
          value: value.value ?? "",
          editable: value.editable,
        })),
      ),
    })),
  };
}
