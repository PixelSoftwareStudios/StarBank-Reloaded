import { invoke } from "@tauri-apps/api/core";

const HEADER_BYTES = 5;
const CONTROL_ONLY = 0;
const WITH_ATTACHMENT = 1;
const CONTROL_LENGTH_OFFSET = 1;
const UNPAIRED_SURROGATE = /[\uD800-\uDFFF]/u;
const encoder = new TextEncoder();
const decoder = new TextDecoder("utf-8", { fatal: true, ignoreBOM: true });

interface FileDialogReply {
  name: string;
  control: string;
}

interface NativeReply {
  control: string;
  bytes?: Uint8Array;
}

export function pickBank(control: string): Promise<FileDialogReply | null> {
  return invoke<FileDialogReply | null>("open_bank", { control });
}

export function canExportBank(): Promise<boolean> {
  return invoke<boolean>("can_export_bank");
}

export function exportBankCopy(
  control: string,
  suggestedName: string,
): Promise<FileDialogReply | null> {
  return invoke<FileDialogReply | null>("export_bank_copy", {
    control,
    suggestedName,
  });
}

export async function dispatchBank(
  control: string,
  bytes?: Uint8Array,
): Promise<NativeReply> {
  const encoded = encoder.encode(control);
  const buffer = new Uint8Array(
    HEADER_BYTES + encoded.length + (bytes?.length ?? 0),
  );
  const frame = new DataView(buffer.buffer);
  frame.setUint8(0, bytes === undefined ? CONTROL_ONLY : WITH_ATTACHMENT);
  frame.setUint32(CONTROL_LENGTH_OFFSET, encoded.length, true);
  buffer.set(encoded, HEADER_BYTES);
  if (bytes !== undefined) {
    buffer.set(bytes, HEADER_BYTES + encoded.length);
  }

  const reply = await invoke<ArrayBuffer>("dispatch_bank", buffer);
  if (!(reply instanceof ArrayBuffer) || reply.byteLength < HEADER_BYTES) {
    throw new Error("The bank engine sent an invalid native reply.");
  }
  const returned = new DataView(reply);
  const flag = returned.getUint8(0);
  const controlEnd =
    HEADER_BYTES + returned.getUint32(CONTROL_LENGTH_OFFSET, true);
  if (
    controlEnd > reply.byteLength ||
    (flag !== WITH_ATTACHMENT &&
      (flag !== CONTROL_ONLY || controlEnd !== reply.byteLength))
  ) {
    throw new Error("The bank engine sent an invalid native reply.");
  }
  return {
    control: decoder.decode(
      new Uint8Array(reply, HEADER_BYTES, controlEnd - HEADER_BYTES),
    ),
    bytes:
      flag === WITH_ATTACHMENT ? new Uint8Array(reply, controlEnd) : undefined,
  };
}

export function decodeBankXml(bytes: Uint8Array): string {
  return decoder.decode(bytes);
}

export function encodeBankXml(xml: string): Uint8Array {
  if (UNPAIRED_SURROGATE.test(xml)) {
    throw new Error("The bank XML contains invalid Unicode text.");
  }
  return encoder.encode(xml);
}
