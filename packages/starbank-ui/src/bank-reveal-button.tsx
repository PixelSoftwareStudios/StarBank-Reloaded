import { FolderOpen } from "lucide-react";
import type { EditorAction, EditorTarget } from "starbank-editor";
import type { RunEditorAction } from "./editor-actions";

export function BankRevealButton({
  reveal,
  target,
  busy,
  run,
}: {
  reveal?: EditorAction;
  target: EditorTarget;
  busy: boolean;
  run: RunEditorAction;
}) {
  const unavailable = "Open in Folder is unavailable on this platform.";
  return (
    <button
      className="editor-button"
      type="button"
      aria-label="Open in Folder"
      aria-description={reveal ? undefined : unavailable}
      aria-disabled={!reveal || busy}
      data-disabled={!reveal || busy ? true : undefined}
      title={reveal ? "Open in Folder" : unavailable}
      onClick={() => {
        if (reveal && !busy) run(() => reveal(target));
      }}
    >
      <FolderOpen aria-hidden="true" className="settings-icon" />
    </button>
  );
}
