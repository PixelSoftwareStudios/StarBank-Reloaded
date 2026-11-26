import { AppShell } from "@starbank/ui";
import { createRoot } from "react-dom/client";
import "@starbank/ui/styles.css";

const root = document.getElementById("root");
if (!root) {
  throw new Error("Missing StarBank application root");
}

createRoot(root).render(<AppShell name="StarBank Web" />);
