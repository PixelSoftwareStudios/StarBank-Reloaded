export interface ImportedBank {
  readonly name: string;
  readonly bytes: ArrayBuffer;
}

export interface BankDownload {
  readonly href: string;
  readonly name: string;
  release(): void;
}

/** Reads a user-selected file after applying the engine's byte bound. */
export async function readBankFile(
  file: File,
  maxBytes: number,
): Promise<ImportedBank> {
  if (!Number.isSafeInteger(maxBytes) || maxBytes < 1) {
    throw new RangeError(
      "The bank engine supplied an invalid file size limit.",
    );
  }
  if (file.size > maxBytes) {
    throw new RangeError("This bank exceeds the engine's file size limit.");
  }
  return { name: file.name, bytes: await file.arrayBuffer() };
}

/**
 * Builds a local link for a user-initiated export. Release it after replacing or closing the link.
 * The filename must already match the export's signing context.
 */
export function prepareBankDownload(
  bytes: ArrayBuffer,
  name: string,
): BankDownload {
  if (
    !name ||
    name === "." ||
    name === ".." ||
    name.includes("/") ||
    name.includes("\\") ||
    [...name].some((character) => character.charCodeAt(0) < 32)
  ) {
    throw new Error(
      "Choose a download filename without path separators or control characters.",
    );
  }

  const href = URL.createObjectURL(
    new Blob([bytes], { type: "application/octet-stream" }),
  );
  return { href, name, release: () => URL.revokeObjectURL(href) };
}

/** Packages the exact opening bytes as a timestamped backup download. */
export function prepareOpeningBankBackup(
  bytes: ArrayBuffer,
  sourceName: string,
  now = new Date(),
): BankDownload {
  const stem = sourceName.replace(/\.(?:sc2bank|xml)$/i, "");
  if (!stem) throw new Error("The opening bank does not have a usable name.");
  const [date, time] = now.toISOString().split("T");
  const day = date.replaceAll("-", "");
  const clock = time.split(".")[0].replaceAll(":", "");
  return prepareBankDownload(bytes, `${stem}_${day}_${clock}.sc2bank.bak`);
}

/** Calls the clipboard API during the user gesture even when an engine response is pending. */
export async function writeBankClipboard(
  text: string | Promise<string>,
  clipboard: Clipboard | undefined = navigator.clipboard,
): Promise<void> {
  if (!clipboard) {
    throw new Error("Clipboard access is unavailable.");
  }
  if (typeof text === "string") {
    await clipboard.writeText(text);
    return;
  }
  if (
    typeof ClipboardItem !== "function" ||
    typeof clipboard.write !== "function"
  ) {
    throw new Error("Copy the prepared XML after it is available.");
  }
  const plainText = text.then((xml) => new Blob([xml], { type: "text/plain" }));
  await clipboard.write([new ClipboardItem({ "text/plain": plainText })]);
}
