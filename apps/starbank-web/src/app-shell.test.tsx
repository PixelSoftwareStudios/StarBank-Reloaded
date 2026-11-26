import { AppShell } from "@starbank/ui";
import { renderToStaticMarkup } from "react-dom/server";
import { expect, it } from "vitest";

it("renders a truthful empty workspace before engine integration", () => {
  const markup = renderToStaticMarkup(<AppShell name="StarBank Web" />);

  expect(markup).toContain("StarBank Web");
  expect(markup).toContain("Map List");
  expect(markup).toContain("No maps loaded.");
  expect(markup).toContain("Bank editing is unavailable in this build.");
  expect(markup).not.toContain("<button");
  expect(markup).not.toContain("<input");
});
