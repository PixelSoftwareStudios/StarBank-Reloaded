import type {
  EngineCommand,
  EngineReply,
  EngineSnapshot,
} from "@starbank/engine-client/protocol";
import {
  decodeEngineReply,
  engineRequest,
} from "@starbank/engine-client/protocol";
import type {
  BankEditorController,
  BankEditorDocument,
  EditorActionResult,
  EditorRow,
  EditorTarget,
} from "@starbank/ui";
import { AppShell } from "@starbank/ui";
import { useEffect, useRef, useState } from "react";
import { createRoot } from "react-dom/client";
import {
  canExportBank,
  decodeBankXml,
  dispatchBank,
  encodeBankXml,
  exportBankCopy,
  pickBank,
} from "./transport";
import "@starbank/ui/styles.css";

const APP_NAME = "StarBank Reloaded";

interface DesktopDocument {
  projection: BankEditorDocument;
  canUndo: boolean;
  canRedo: boolean;
}

function DesktopApp() {
  const [document, setDocument] = useState<DesktopDocument>();
  const [busy, setBusy] = useState(false);
  const [exportAvailable, setExportAvailable] = useState(false);
  const inProgress = useRef(false);
  const nextRequest = useRef(0);

  useEffect(() => {
    void canExportBank().then(setExportAvailable, () => {});
  }, []);

  function requestId() {
    nextRequest.current += 1;
    return `desktop-${nextRequest.current}`;
  }

  async function action(
    run: () => Promise<EditorActionResult>,
  ): Promise<EditorActionResult> {
    if (inProgress.current) {
      return { ok: false, message: "A bank operation is already in progress." };
    }
    inProgress.current = true;
    setBusy(true);
    try {
      return await run();
    } catch (error) {
      return { ok: false, message: messageFor(error) };
    } finally {
      inProgress.current = false;
      setBusy(false);
    }
  }

  async function dispatch(
    target: EditorTarget,
    command: EngineCommand,
    bytes?: Uint8Array,
  ) {
    const id = requestId();
    const control = engineRequest(id, command, {
      document_id: target.id,
      expected_revision: target.revision,
    });
    const nativeReply = await dispatchBank(JSON.stringify(control), bytes);
    const reply = decodeEngineReply(nativeReply.control, id);
    if (reply.result.kind === "error") {
      throw new Error(reply.result.payload.message);
    }
    return { reply, bytes: nativeReply.bytes };
  }

  async function adopt(name: string, reply: EngineReply) {
    if (reply.result.kind === "error") {
      throw new Error(reply.result.payload.message);
    }
    if (
      reply.result.kind !== "snapshot" ||
      reply.document_id === null ||
      reply.revision === null
    ) {
      throw new Error("The bank engine did not return a document.");
    }
    const target = { id: reply.document_id, revision: reply.revision };
    const snapshot = reply.result.payload;
    const raw = await dispatch(target, { kind: "raw" });
    if (raw.reply.result.kind !== "raw" || raw.bytes === undefined) {
      throw new Error("The bank engine did not return XML.");
    }
    setDocument({
      projection: {
        ...target,
        name,
        dirty: snapshot.dirty,
        sections: displaySections(snapshot),
        signatureStatus: snapshot.signature_status,
        xml: decodeBankXml(raw.bytes),
      },
      canUndo: snapshot.can_undo,
      canRedo: snapshot.can_redo,
    });
  }

  function update(
    target: EditorTarget,
    command: EngineCommand,
    bytes?: Uint8Array,
  ) {
    return action(async () => {
      if (!document || document.projection.id !== target.id) {
        return { ok: false, message: "Open a bank before editing it." };
      }
      const { reply } = await dispatch(target, command, bytes);
      await adopt(document.projection.name, reply);
      return { ok: true };
    });
  }

  const controller: BankEditorController = {
    document: document?.projection,
    busy,
    onOpen: () =>
      action(async () => {
        const id = requestId();
        const control = engineRequest(id, { kind: "open" });
        const selected = await pickBank(JSON.stringify(control));
        if (selected) {
          await adopt(selected.name, decodeEngineReply(selected.control, id));
          return { ok: true, message: `Opened ${selected.name}.` };
        }
        return { ok: true };
      }),
    onEditValues: (target, edits) =>
      update(target, { kind: "set_values", payload: { edits } }),
    onReplaceXml: (target, xml) =>
      update(target, { kind: "replace_raw" }, encodeBankXml(xml)),
    onReset: (target) => update(target, { kind: "reset" }),
    onUndo: document?.canUndo
      ? (target) => update(target, { kind: "undo" })
      : undefined,
    onRedo: document?.canRedo
      ? (target) => update(target, { kind: "redo" })
      : undefined,
    onExport: exportAvailable
      ? (target) =>
          action(async () => {
            if (!document || document.projection.id !== target.id) {
              throw new Error("Open a bank before exporting it.");
            }
            const id = requestId();
            const control = engineRequest(
              id,
              { kind: "prepare" },
              {
                document_id: target.id,
                expected_revision: target.revision,
              },
            );
            const exported = await exportBankCopy(
              JSON.stringify(control),
              document.projection.name,
            );
            if (!exported) return { ok: true };
            const { result } = decodeEngineReply(exported.control, id);
            if (result.kind === "error")
              throw new Error(result.payload.message);
            if (result.kind !== "prepared") {
              throw new Error("The bank engine did not confirm the export.");
            }
            return { ok: true, message: `Exported ${exported.name}.` };
          })
      : undefined,
    onCopy: navigator.clipboard
      ? (target) =>
          action(async () => {
            const { reply, bytes } = await dispatch(target, {
              kind: "prepare",
            });
            if (reply.result.kind !== "prepared" || bytes === undefined) {
              throw new Error("The bank engine did not prepare XML.");
            }
            const status = reply.result.payload.signature_status;
            if (
              status !== "valid" &&
              status !== "unsigned" &&
              !window.confirm(
                "Copy XML with an unverified signature? StarCraft II may refuse it.",
              )
            ) {
              return { ok: true };
            }
            await navigator.clipboard.writeText(decodeBankXml(bytes));
            return { ok: true, message: "Copied bank XML." };
          })
      : undefined,
  };
  return <AppShell name={APP_NAME} editor={controller} />;
}

function displaySections(snapshot: EngineSnapshot) {
  return snapshot.sections.map((section) => ({
    id: section.id,
    name: section.name ?? "Unnamed section",
    rows: section.keys.flatMap((key): EditorRow[] => {
      const name = key.name ?? "Unnamed key";
      if (key.values.length === 0) {
        return [{ id: key.id, name, type: "", value: "", editable: false }];
      }
      return key.values.map((value) => ({
        id: value.id,
        name,
        childName: value.name === "Value" ? undefined : value.name,
        type: value.kind ?? "",
        value: value.value ?? "",
        editable: value.editable,
      }));
    }),
  }));
}

function messageFor(error: unknown): string {
  if (error instanceof Error) return error.message;
  if (typeof error === "string" && error === "dialog_busy") {
    return "A file dialog is already open.";
  }
  if (error === "write_uncertain") {
    return "The export outcome is uncertain. Inspect the destination before retrying.";
  }
  if (
    typeof error === "object" &&
    error !== null &&
    "write_rejected" in error &&
    typeof error.write_rejected === "string"
  ) {
    return `The export was refused. ${error.write_rejected}`;
  }
  if (typeof error === "object" && error !== null && "file" in error) {
    if (error.file === "too_large") return "The selected bank is too large.";
    return "Select a readable StarCraft II bank file.";
  }
  return "The native bank operation failed.";
}

const root = document.getElementById("root");
if (!root) {
  throw new Error("Missing StarBank application root");
}
createRoot(root).render(<DesktopApp />);
