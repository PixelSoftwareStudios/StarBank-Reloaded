import { createRoot } from "react-dom/client";
import { StarBankWeb } from "./browser-app";
import "@starbank/ui/styles.css";

const root = document.getElementById("root");
if (!root) {
  throw new Error("Missing StarBank application root");
}

createRoot(root).render(<StarBankWeb />);
