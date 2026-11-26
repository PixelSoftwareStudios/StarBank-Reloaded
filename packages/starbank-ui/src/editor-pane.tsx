import { type ReactNode, useRef, useState } from "react";
import type { BankEditorController } from "starbank-editor";
import { BankEditor, type EditorSettings } from "./bank-editor";
import {
  fileDropHandlers,
  OpenBankButton,
  type RunEditorAction,
  SettingsButton,
} from "./editor-actions";

export function EditorPane({
  name,
  controller,
  settings,
  onOpenSettings,
  tools,
}: {
  name: string;
  controller: BankEditorController;
  settings?: EditorSettings;
  onOpenSettings?: () => void;
  tools?: ReactNode;
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

  if (controller.document) {
    return (
      <BankEditor
        key={controller.document.id}
        controller={controller}
        document={controller.document}
        busy={busy}
        notice={notice}
        clearNotice={() => setNotice(undefined)}
        run={run}
        settings={settings}
        onOpenSettings={onOpenSettings}
        tools={tools}
      />
    );
  }

  return (
    <div
      className="editor-welcome"
      {...fileDropHandlers(controller.onOpenFile, busy, run)}
    >
      <h1 className="text-[1.75rem] leading-tight font-semibold">{name}</h1>
      <div className="editor-open-actions">
        <OpenBankButton controller={controller} disabled={busy} run={run} />
        {tools}
        <SettingsButton open={onOpenSettings} />
      </div>
      <p className="empty-message" role="status">
        {busy
          ? "Opening bank…"
          : notice || controller.status || "No bank open."}
      </p>
    </div>
  );
}
