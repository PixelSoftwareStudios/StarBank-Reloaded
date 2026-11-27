import { type ReactNode, useRef, useState } from "react";
import { type MapContext, MapContextDetails } from "../inventory/map-context";
import { BankEditor, type EditorSettings } from "./bank-editor";
import {
  fileDropHandlers,
  OpenBankButton,
  type RunEditorAction,
  SettingsButton,
} from "./editor-actions";
import type { BankEditorController } from "./index";

export function EditorPane({
  name,
  controller,
  settings,
  onOpenSettings,
  tools,
  mapContext,
}: {
  name: string;
  controller: BankEditorController;
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
        mapContext={mapContext}
      />
    );
  }

  return (
    <div
      className="editor-welcome-frame"
      {...fileDropHandlers(controller.onOpenFile, busy, run)}
    >
      <div className="editor-header-bar">
        <div className="editor-open-actions">
          <OpenBankButton controller={controller} disabled={busy} run={run} />
          {tools}
          <SettingsButton open={onOpenSettings} />
        </div>
      </div>
      <div className="editor-welcome">
        <h1 className="text-[1.75rem] leading-tight font-semibold tracking-tight">
          {mapContext?.map.name ?? name}
        </h1>
        {mapContext && (
          <MapContextDetails context={mapContext} disabled={busy} />
        )}
        <p className="empty-message" role="status">
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
  );
}
