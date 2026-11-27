import { useState } from "react";
import { useStore } from "zustand";
import { createStore } from "zustand/vanilla";
import type { EditorDraft } from "./editor-model";
import type { EditorProfileDraft } from "./simple-profile-model";

type EditorView = "simple" | "table" | "raw";
type DraftUpdater<Draft> =
  | Draft
  | undefined
  | ((draft: Draft | undefined) => Draft | undefined);

interface EditorUiState {
  draft?: EditorDraft;
  profileDraft?: EditorProfileDraft;
  view?: EditorView;
  setDraft: (update: DraftUpdater<EditorDraft>) => void;
  setProfileDraft: (update: DraftUpdater<EditorProfileDraft>) => void;
  setView: (view: EditorView) => void;
}

function createEditorState() {
  return createStore<EditorUiState>()((set) => ({
    setDraft: (update) =>
      set(({ draft }) => ({
        draft: typeof update === "function" ? update(draft) : update,
      })),
    setProfileDraft: (update) =>
      set(({ profileDraft }) => ({
        profileDraft:
          typeof update === "function" ? update(profileDraft) : update,
      })),
    setView: (view) => set({ view }),
  }));
}

/** The editor mounts this store per document. Accepted values come from the engine. */
export function useEditorState(): EditorUiState {
  const [store] = useState(createEditorState);
  return useStore(store);
}
