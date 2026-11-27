import { type ReactNode, useRef, useState } from "react";
import { type MapContext, MapContextDetails } from "../inventory/map-context";
import { buttonClasses } from "../shared/control-styles";
import { BankEditor, type EditorSettings } from "./bank-editor";
import {
  fileDropHandlers,
  OpenBankButton,
  type RunEditorAction,
  SettingsButton,
} from "./editor-actions";
import type { BankEditorController, EditorActionResult } from "./editor-model";

export type OpenRecovery =
  | {
      kind?: "accepted_open";
      name: string;
      finish?: () => Promise<EditorActionResult>;
    }
  | { kind: "verified_write"; reopen: () => Promise<EditorActionResult> };

export function EditorPane({
  name,
  controller,
  openRecovery,
  settings,
  onOpenSettings,
  tools,
  mapContext,
}: {
  name: string;
  controller: BankEditorController;
  openRecovery?: OpenRecovery;
  settings?: EditorSettings;
  onOpenSettings?: () => void;
  tools?: ReactNode;
  mapContext?: MapContext;
}) {
  const running = useRef(false);
  const [working, setWorking] = useState(false);
  const [notice, setNotice] = useState<string>();
  const busy = working || Boolean(controller.busy);
  const run: RunEditorAction = (action, onSuccess) => {
    if (running.current || controller.busy) return;
    running.current = true;
    setWorking(true);
    setNotice(undefined);

    void (async () => {
      try {
        const result = await action();
        if (result.ok) onSuccess?.();
        setNotice(result.message);
      } catch (error) {
        setNotice(
          error instanceof Error
            ? error.message
            : "The action could not be completed.",
        );
      } finally {
        running.current = false;
        setWorking(false);
      }
    })();
  };

  const verifiedWrite = openRecovery?.kind === "verified_write";
  const recover = verifiedWrite ? openRecovery.reopen : openRecovery?.finish;
  const recovery = openRecovery && (
    <div className="flex flex-wrap items-center gap-2 border-b border-line px-3.5 py-2 text-[0.875rem]">
      <p role="status" className="min-w-0 flex-1 wrap-anywhere">
        {verifiedWrite
          ? "The bank write was verified on disk. Reopen the bank to resume editing."
          : openRecovery.finish
            ? `The engine opened ${openRecovery.name}. Finish opening it to display the bank.`
            : `The engine stopped before ${openRecovery.name} could be displayed.`}
        {controller.document && " The previous bank is read-only."}
      </p>
      {recover && (
        <button
          className={buttonClasses()}
          type="button"
          disabled={busy}
          onClick={() => run(recover)}
        >
          {verifiedWrite ? "Reopen bank…" : "Finish opening bank"}
        </button>
      )}
    </div>
  );

  if (controller.document) {
    return (
      <>
        {recovery}
        <BankEditor
          key={controller.document.id}
          controller={
            openRecovery
              ? { ...controller, snapshotUnavailable: true }
              : controller
          }
          document={controller.document}
          busy={busy}
          notice={notice}
          clearNotice={() => setNotice(undefined)}
          run={run}
          settings={settings}
          onOpenSettings={onOpenSettings}
          tools={tools}
          mapContext={mapContext}
        />
      </>
    );
  }

  const openDisabled = busy || openRecovery !== undefined;
  return (
    <>
      {recovery}
      <div
        className="editor-welcome-frame grid h-full min-h-[17rem] grid-rows-[auto_minmax(0,1fr)] p-3.5"
        {...fileDropHandlers(controller.onOpenFile, openDisabled, run)}
      >
        <div className="editor-header-bar flex min-w-0 items-center gap-1.5 justify-end">
          <div className="editor-open-actions ms-auto flex flex-none items-center justify-end gap-1.5">
            <OpenBankButton
              controller={controller}
              disabled={openDisabled}
              run={run}
            />
            {tools}
            <SettingsButton open={onOpenSettings} />
          </div>
        </div>
        <div className="editor-welcome grid min-h-0 place-content-center justify-items-center gap-3 px-6 py-8 text-center">
          <h1 className="text-[1.75rem] leading-tight font-semibold tracking-tight">
            {mapContext?.map.name ?? name}
          </h1>
          {mapContext && (
            <MapContextDetails context={mapContext} disabled={openDisabled} />
          )}
          <p className="empty-message text-muted" role="status">
            {busy
              ? "Opening bank…"
              : notice ||
                controller.status ||
                (mapContext && mapContext.choices.length === 0
                  ? "No bank files associated with this map."
                  : "No bank open.")}
          </p>
        </div>
      </div>
    </>
  );
}
