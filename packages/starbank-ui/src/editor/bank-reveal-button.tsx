import { FolderOpen } from "lucide-react";
import type { EditorAction, EditorTarget } from "starbank-editor";
import { EditorHelpButton } from "../shared/editor-tooltip";
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
    <EditorHelpButton
      className="editor-button"
      aria-label="Open in Folder"
      aria-description={reveal ? undefined : unavailable}
      aria-disabled={!reveal || busy}
      data-disabled={!reveal || busy ? true : undefined}
      help={reveal ? "Reveal the open bank in the file manager" : unavailable}
      placement="bottom"
      onClick={() => {
        if (reveal && !busy) run(() => reveal(target));
      }}
    >
      <FolderOpen aria-hidden="true" className="settings-icon" />
    </EditorHelpButton>
  );
}
