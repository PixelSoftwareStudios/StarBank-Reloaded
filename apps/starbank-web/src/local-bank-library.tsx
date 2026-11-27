import { type ChangeEvent, useRef, useState } from "react";
import type { EditorActionResult } from "starbank-ui";
import { buttonClasses } from "starbank-ui/controls";
import {
  type BankSelectionSource,
  collectLocalBanks,
  type LocalBank,
  type LocalBankSelection,
  MAX_LOCAL_BANKS,
  MAX_SELECTED_FILES,
} from "./local-bank-selection";

const BANK_PAGE_SIZE = 80;
const BANK_FILE_TYPES = ".sc2bank,.xml";

interface LocalBankLibraryProps {
  readonly maxBankBytes: number;
  readonly openedFile?: File;
  readonly switchingBlocked: boolean;
  readonly hasDraft: boolean;
  readonly open: (file: File) => Promise<EditorActionResult>;
}

function selectionNotice(selection: LocalBankSelection): string {
  const count = selection.banks.length;
  const summary = count
    ? `${count} local bank${count === 1 ? "" : "s"} listed.`
    : "No supported banks found in this selection.";
  const issues = [
    selection.ignored > 0
      ? `${selection.ignored} unrelated file${selection.ignored === 1 ? "" : "s"} skipped.`
      : undefined,
    selection.oversized > 0
      ? `${selection.oversized} oversized bank${selection.oversized === 1 ? "" : "s"} skipped.`
      : undefined,
    selection.truncated
      ? `Selection limited to ${MAX_LOCAL_BANKS} banks and ${MAX_SELECTED_FILES} inspected files.`
      : undefined,
  ];
  return [summary, ...issues].filter(Boolean).join(" ");
}

export function LocalBankLibrary({
  maxBankBytes,
  openedFile,
  switchingBlocked,
  hasDraft,
  open,
}: LocalBankLibraryProps) {
  const filesInput = useRef<HTMLInputElement>(null);
  const folderInput = useRef<HTMLInputElement>(null);
  const [banks, setBanks] = useState<readonly LocalBank[]>([]);
  const [notice, setNotice] = useState("");
  const [limit, setLimit] = useState(BANK_PAGE_SIZE);
  const supportsFolder = "webkitdirectory" in HTMLInputElement.prototype;

  function importSelection(
    event: ChangeEvent<HTMLInputElement>,
    source: BankSelectionSource,
  ) {
    const input = event.currentTarget;
    if (input.files?.length) {
      const collected = collectLocalBanks(input.files, source, maxBankBytes);
      if (collected.banks.length) {
        setBanks(collected.banks);
        setLimit(BANK_PAGE_SIZE);
      }
      setNotice(selectionNotice(collected));
    }
    input.value = "";
  }

  async function openSelection(file: File) {
    setNotice("");
    try {
      const result = await open(file);
      setNotice(result.message ?? "");
    } catch (error) {
      setNotice(
        error instanceof Error
          ? error.message
          : "This bank could not be opened.",
      );
    }
  }

  return (
    <details className="local-bank-library border-be border-line bg-surface px-4 py-[0.45rem]">
      <summary className="w-fit cursor-pointer font-semibold">
        Local banks
      </summary>
      <div className="grid gap-2 pbs-2 pbe-[0.2rem] text-[0.875rem]">
        <p className="text-muted">
          Choose files or a folder to replace the list. Select a bank to open
          it.
        </p>
        <div className="flex flex-wrap gap-[0.35rem]">
          <input
            ref={filesInput}
            type="file"
            multiple
            accept={BANK_FILE_TYPES}
            aria-label="Select local bank files"
            hidden
            onChange={(event) => importSelection(event, "files")}
          />
          <button
            className={buttonClasses()}
            type="button"
            onClick={() => filesInput.current?.click()}
          >
            Select bank files…
          </button>
          {supportsFolder && (
            <>
              <input
                ref={(input) => {
                  folderInput.current = input;
                  if (input) input.webkitdirectory = true;
                }}
                type="file"
                multiple
                aria-label="Select local bank folder"
                hidden
                onChange={(event) => importSelection(event, "folder")}
              />
              <button
                className={buttonClasses()}
                type="button"
                onClick={() => folderInput.current?.click()}
              >
                Select bank folder…
              </button>
            </>
          )}
          {banks.length > 0 && (
            <button
              className={buttonClasses()}
              type="button"
              onClick={() => {
                setBanks([]);
                setNotice("Bank list cleared.");
              }}
            >
              Clear list
            </button>
          )}
        </div>
        {notice && (
          <p className="text-muted" role="status">
            {notice}
          </p>
        )}
        {hasDraft && banks.length > 0 && (
          <p className="text-muted">
            Apply or discard your draft before opening another bank.
          </p>
        )}
        {banks.length > 0 && (
          <ul
            className="flex max-block-40 flex-wrap gap-[0.35rem] overflow-auto p-[0.15rem]"
            aria-label="Local bank list"
          >
            {banks.slice(0, limit).map(({ file, label }, index) => (
              <li className="max-inline-full" key={`${index}:${label}`}>
                <button
                  className={`${buttonClasses()} max-inline-full wrap-anywhere aria-[current=true]:bg-selected`}
                  type="button"
                  title={label}
                  aria-current={file === openedFile ? "true" : undefined}
                  disabled={switchingBlocked || file === openedFile}
                  onClick={() => void openSelection(file)}
                >
                  {label}
                </button>
              </li>
            ))}
          </ul>
        )}
        {banks.length > limit && (
          <button
            className={buttonClasses()}
            type="button"
            onClick={() => setLimit(limit + BANK_PAGE_SIZE)}
          >
            Show more local banks
          </button>
        )}
      </div>
    </details>
  );
}
