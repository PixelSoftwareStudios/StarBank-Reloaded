import { defaultKeymap, history, historyKeymap } from "@codemirror/commands";
import { xml } from "@codemirror/lang-xml";
import { HighlightStyle, syntaxHighlighting } from "@codemirror/language";
import { searchKeymap } from "@codemirror/search";
import {
  Annotation,
  Compartment,
  EditorState,
  Transaction,
} from "@codemirror/state";
import {
  drawSelection,
  EditorView,
  keymap,
  lineNumbers,
} from "@codemirror/view";
import { tags } from "@lezer/highlight";
import { type Ref, useEffect, useImperativeHandle, useRef } from "react";
import { rawSyntaxDiagnostics } from "./raw-diagnostics";

const externalUpdate = Annotation.define<boolean>();
const editorTheme = EditorView.theme({
  "&": {
    height: "100%",
    minHeight: "0",
    border: "1px solid var(--shell-divider)",
    borderRadius: "var(--control-radius)",
    background: "var(--map-field-bg)",
    overflow: "hidden",
  },
  ".cm-scroller": {
    overflow: "auto",
    fontFamily: "ui-monospace, monospace",
  },
  ".cm-gutters": {
    color: "var(--shell-muted)",
    background: "var(--map-pane-bg)",
    borderColor: "var(--shell-divider)",
  },
  ".cm-selectionLayer .cm-selectionBackground": {
    background: "var(--map-selected-bg)",
  },
  ".cm-cursor": { borderColor: "var(--shell-fg)" },
  ".cm-panels": {
    color: "var(--shell-fg)",
    background: "var(--map-pane-bg)",
  },
});
const highlighting = HighlightStyle.define([
  { tag: [tags.tagName, tags.angleBracket], color: "var(--xml-tag)" },
  { tag: tags.attributeName, color: "var(--xml-attribute)" },
  { tag: [tags.attributeValue, tags.string], color: "var(--xml-value)" },
  { tag: tags.comment, color: "var(--shell-muted)", fontStyle: "italic" },
]);

function lineSeparator(value: string) {
  if (value.includes("\r\n") && !/\r(?!\n)|(^|[^\r])\n/.test(value))
    return "\r\n";
  if (value.includes("\r") && !value.includes("\n")) return "\r";
  return "\n";
}

function editingMode(readOnly: boolean) {
  return [EditorState.readOnly.of(readOnly), EditorView.editable.of(!readOnly)];
}

export interface RawEditorHandle {
  focus: () => void;
}

export default function RawEditor({
  ref,
  value,
  readOnly,
  onChange,
  wrap = false,
}: {
  ref?: Ref<RawEditorHandle>;
  value: string;
  readOnly: boolean;
  onChange: (value: string) => void;
  wrap?: boolean;
}) {
  const host = useRef<HTMLDivElement>(null);
  const instance = useRef<EditorView>(null);
  const currentValue = useRef(value);
  const mode = useRef(new Compartment());
  const wrapping = useRef(new Compartment());
  const onChangeRef = useRef(onChange);

  useImperativeHandle(
    ref,
    () => ({ focus: () => instance.current?.focus() }),
    [],
  );

  useEffect(() => {
    onChangeRef.current = onChange;
  }, [onChange]);

  useEffect(() => {
    if (!host.current) return;
    const view = new EditorView({
      doc: currentValue.current,
      parent: host.current,
      extensions: [
        EditorState.lineSeparator.of(lineSeparator(currentValue.current)),
        xml(),
        rawSyntaxDiagnostics,
        lineNumbers(),
        drawSelection(),
        history(),
        keymap.of([...defaultKeymap, ...historyKeymap, ...searchKeymap]),
        editorTheme,
        syntaxHighlighting(highlighting),
        EditorView.contentAttributes.of({ "aria-label": "Bank XML" }),
        mode.current.of(editingMode(true)),
        wrapping.current.of([]),
        EditorView.updateListener.of((update) => {
          if (
            update.docChanged &&
            !update.transactions.some((tx) => tx.annotation(externalUpdate))
          ) {
            const text = update.state.sliceDoc();
            currentValue.current = text;
            onChangeRef.current(text);
          }
        }),
      ],
    });
    instance.current = view;
    return () => {
      instance.current = null;
      view.destroy();
    };
  }, []);

  useEffect(() => {
    const view = instance.current;
    if (view && currentValue.current !== value) {
      currentValue.current = value;
      view.dispatch({
        changes: { from: 0, to: view.state.doc.length, insert: value },
        annotations: [
          externalUpdate.of(true),
          Transaction.addToHistory.of(false),
        ],
      });
    }
  }, [value]);

  useEffect(() => {
    instance.current?.dispatch({
      effects: mode.current.reconfigure(editingMode(readOnly)),
    });
  }, [readOnly]);

  useEffect(() => {
    instance.current?.dispatch({
      effects: wrapping.current.reconfigure(
        wrap ? EditorView.lineWrapping : [],
      ),
    });
  }, [wrap]);

  return <div className="editor-raw h-full min-h-0" ref={host} />;
}
