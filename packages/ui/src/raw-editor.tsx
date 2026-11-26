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
import { useEffect, useRef } from "react";

const externalUpdate = Annotation.define<boolean>();
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

export default function RawEditor({
  value,
  readOnly,
  onChange,
}: {
  value: string;
  readOnly: boolean;
  onChange: (value: string) => void;
}) {
  const host = useRef<HTMLDivElement>(null);
  const instance = useRef<EditorView>(null);
  const currentValue = useRef(value);
  const mode = useRef(new Compartment());
  const onChangeRef = useRef(onChange);

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
        lineNumbers(),
        drawSelection(),
        history(),
        keymap.of([...defaultKeymap, ...historyKeymap, ...searchKeymap]),
        syntaxHighlighting(highlighting),
        EditorView.contentAttributes.of({ "aria-label": "Bank XML" }),
        mode.current.of(editingMode(true)),
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

  return <div className="editor-raw" ref={host} />;
}
