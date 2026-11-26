type BankFileEncoding = "utf-8" | "utf-16le" | "utf-16be";

const decoderOptions = { fatal: true, ignoreBOM: true };
const utf8 = new TextDecoder("utf-8", decoderOptions);
const utf16le = new TextDecoder("utf-16le", decoderOptions);
const utf16be = new TextDecoder("utf-16be", decoderOptions);
const encoder = new TextEncoder();
const LE_MARK = [0xff, 0xfe] as const;
const BE_MARK = [0xfe, 0xff] as const;
const BYTE_ORDER_MARK = "\uFEFF";
const UTF16_UNIT_BYTES = Uint16Array.BYTES_PER_ELEMENT;
const UNPAIRED_SURROGATE = /[\uD800-\uDFFF]/u;

/** Decodes validated file bytes for a text clipboard, preserving the lexical BOM. */
export function decodeBankFileXml(bytes: ArrayBuffer | Uint8Array): string {
  const view = bytes instanceof Uint8Array ? bytes : new Uint8Array(bytes);
  if (view[0] === LE_MARK[0] && view[1] === LE_MARK[1]) {
    return utf16le.decode(view);
  }
  if (view[0] === BE_MARK[0] && view[1] === BE_MARK[1]) {
    return utf16be.decode(view);
  }
  return utf8.decode(view);
}

/** Encodes editor text for transport or rebuilds a validated recovery copy. */
export function encodeBankFileXml(
  xml: string,
  encoding: BankFileEncoding,
): ArrayBuffer {
  if (UNPAIRED_SURROGATE.test(xml)) {
    throw new Error("The bank XML contains invalid Unicode text.");
  }
  if (encoding === "utf-8") {
    return encoder.encode(xml).buffer;
  }

  const text = xml.startsWith(BYTE_ORDER_MARK) ? xml : BYTE_ORDER_MARK + xml;
  const bytes = new ArrayBuffer(text.length * UTF16_UNIT_BYTES);
  const output = new DataView(bytes);
  const littleEndian = encoding === "utf-16le";
  for (let index = 0; index < text.length; index += 1) {
    output.setUint16(
      index * UTF16_UNIT_BYTES,
      text.charCodeAt(index),
      littleEndian,
    );
  }
  return bytes;
}
