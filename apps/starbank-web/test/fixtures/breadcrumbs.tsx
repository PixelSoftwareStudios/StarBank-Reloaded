import { type AppSettings, defaultSettings } from "@starbank/platform/settings";
import { AppShell, type BankEditorDocument } from "@starbank/ui";
import { useState } from "react";
import { createRoot } from "react-dom/client";
import "@starbank/ui/styles.css";

const ancestors = [
  "/",
  "Synthetic account",
  "A very long directory containing locally selected account folders",
  "Synthetic account",
  "A path with  multiple  spaces",
  "Files chosen in the native picker belong under this test directory",
];
const firstBank = "An assorted custom mission bank.SC2Bank";
const longBank = `A ${"long ".repeat(27)}bank.SC2Bank`;

function bank(name: string, id: string): BankEditorDocument {
  return {
    id,
    revision: "1",
    name,
    breadcrumbs: [...ancestors, name],
    xml: "<Bank/>",
    dirty: false,
    sections: [],
  };
}

function Fixture() {
  const [document, setDocument] = useState(() => bank(firstBank, "fixture-1"));
  const [settings, setSettings] = useState(defaultSettings);
  function change<Key extends Exclude<keyof AppSettings, "version">>(
    key: Key,
    value: AppSettings[Key],
  ) {
    setSettings((current) => ({ ...current, [key]: value }));
  }

  return (
    <AppShell
      name="StarBank Reloaded"
      breadcrumbAvailable
      preferences={{ settings, change }}
      editor={{
        document,
        onOpen: async () => {
          setDocument(bank(longBank, "fixture-2"));
          return { ok: true };
        },
      }}
    />
  );
}

const root = window.document.getElementById("root");
if (!root) throw new Error("Missing fixture root");
createRoot(root).render(<Fixture />);
