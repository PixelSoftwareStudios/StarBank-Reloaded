import { Buffer } from "node:buffer";
import {
  decodeBankFileXml,
  encodeBankFileXml,
} from "starbank-ui/bank-encoding";
import { describe, expect, test } from "vitest";

const text =
  "\uFEFF<?xml version='1.0' encoding='UTF-16'?>\r\n<Bank><!--星😀--></Bank>";

describe("bank text and recovery bytes", () => {
  test("preserves both UTF-16 byte orders, BOM, and exact lexical content", () => {
    const little = Buffer.from(text, "utf16le");
    const big = Buffer.from(little).swap16();
    expect(decodeBankFileXml(little)).toBe(text);
    expect(decodeBankFileXml(big)).toBe(text);
    expect(new Uint8Array(encodeBankFileXml(text, "utf-16le"))).toEqual(
      new Uint8Array(little),
    );
    expect(
      new Uint8Array(encodeBankFileXml(text.slice(1), "utf-16be")),
    ).toEqual(new Uint8Array(big));
  });

  test("retains any UTF-8 BOM and rejects malformed text rather than replacing it", () => {
    const utf8 = Buffer.from(text, "utf8");
    expect(decodeBankFileXml(utf8)).toBe(text);
    expect(new Uint8Array(encodeBankFileXml(text, "utf-8"))).toEqual(
      new Uint8Array(utf8),
    );
    expect(() =>
      decodeBankFileXml(new Uint8Array([0xfe, 0xff, 0xd8, 0])),
    ).toThrow();
    expect(() => encodeBankFileXml("\uD800", "utf-16be")).toThrow(
      "invalid Unicode text",
    );
  });
});
