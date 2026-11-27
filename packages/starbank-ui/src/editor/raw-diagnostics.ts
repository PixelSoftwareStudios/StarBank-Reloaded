import {
  ensureSyntaxTree,
  syntaxTree,
  syntaxTreeAvailable,
} from "@codemirror/language";
import {
  type EditorState,
  type Range,
  StateEffect,
  StateField,
} from "@codemirror/state";
import {
  Decoration,
  type DecorationSet,
  EditorView,
  ViewPlugin,
  type ViewUpdate,
} from "@codemirror/view";

const DEBOUNCE_MS = 250;
const PARSE_BUDGET_MS = 5;
const MAX_SYNC_PARSE_UNITS = 256 * 1024;
const MAX_INSPECTED_NODES = 8_192;
const MAX_SYNTAX_MARKS = 16;
const SYNTAX_HINT = "Possible XML syntax issue";
const ISSUE_CLASS = "cm-xml-syntax-issue";
const ISSUE_LINE_CLASS = "cm-xml-syntax-issue-line";
const issueAttributes = { title: SYNTAX_HINT };
const issueMark = Decoration.mark({
  class: ISSUE_CLASS,
  attributes: issueAttributes,
});
const issueLine = Decoration.line({
  class: ISSUE_LINE_CLASS,
  attributes: issueAttributes,
});

function completeTree(state: EditorState) {
  const length = state.doc.length;
  const tree = syntaxTree(state);
  if (tree.length === length && syntaxTreeAvailable(state)) return tree;
  if (length > MAX_SYNC_PARSE_UNITS) return undefined;
  const parsed = ensureSyntaxTree(state, length, PARSE_BUDGET_MS);
  return parsed?.length === length ? parsed : undefined;
}

function findSyntaxMarks(
  state: EditorState,
  tree: NonNullable<ReturnType<typeof completeTree>>,
): DecorationSet {
  const cursor = tree.cursor();
  const ranges: Range<Decoration>[] = [];
  const markedLines = new Set<number>();
  for (let visited = 0; visited < MAX_INSPECTED_NODES; visited++) {
    if (cursor.type.isError) {
      const { from, to } = cursor;
      if (from < to) {
        ranges.push(issueMark.range(from, to));
      } else {
        // An insertion error can occur at EOF with no character to underline.
        const line = state.doc.lineAt(from).from;
        if (!markedLines.has(line)) {
          markedLines.add(line);
          ranges.push(issueLine.range(line));
        }
      }
    }
    if (ranges.length === MAX_SYNTAX_MARKS || !cursor.next()) break;
  }
  return Decoration.set(ranges, true);
}

const setSyntaxMarks = StateEffect.define<DecorationSet>();
const syntaxMarks = StateField.define<DecorationSet>({
  create: () => Decoration.none,
  update(marks, transaction) {
    if (transaction.docChanged) return Decoration.none;
    return (
      transaction.effects.find((effect) => effect.is(setSyntaxMarks))?.value ??
      marks
    );
  },
  provide: (field) => [
    EditorView.decorations.from(field),
    EditorView.contentAttributes.from(
      field,
      (marks): Record<string, string> =>
        marks.size ? { "aria-description": SYNTAX_HINT } : {},
    ),
  ],
});

const syntaxCheck = ViewPlugin.fromClass(
  class {
    private timer?: number;
    private waitingForParse = false;

    constructor(private readonly view: EditorView) {
      this.schedule();
    }

    update({ docChanged, state, startState }: ViewUpdate) {
      if (
        docChanged ||
        (this.waitingForParse &&
          syntaxTree(state) !== syntaxTree(startState) &&
          syntaxTreeAvailable(state))
      ) {
        this.schedule();
      }
    }

    destroy() {
      window.clearTimeout(this.timer);
    }

    private schedule() {
      this.waitingForParse = false;
      window.clearTimeout(this.timer);
      this.timer = window.setTimeout(() => {
        this.timer = undefined;
        const { state } = this.view;
        const tree = completeTree(state);
        if (!tree) {
          this.waitingForParse = true;
          return;
        }
        const marks = findSyntaxMarks(state, tree);
        if (marks.size)
          this.view.dispatch({ effects: setSyntaxMarks.of(marks) });
      }, DEBOUNCE_MS);
    }
  },
);

// The Rust engine still validates XML and bank data on Apply and Save.
export const rawSyntaxDiagnostics = [
  syntaxMarks,
  syntaxCheck,
  EditorView.baseTheme({
    [`.${ISSUE_CLASS}`]: {
      textDecoration: "underline wavy var(--editor-danger)",
    },
    [`.${ISSUE_LINE_CLASS}`]: {
      boxShadow: "inset 0 -1px var(--editor-danger)",
    },
  }),
];
