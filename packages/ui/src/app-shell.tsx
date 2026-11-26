import { Separator } from "@heroui/react/separator";

export function AppShell({ name }: { name: string }) {
  return (
    <div className="starbank-shell">
      <main className="bank-workspace grid min-w-0 place-items-center px-6 py-8 text-center">
        <div>
          <h1 className="text-[1.75rem] leading-tight font-semibold">{name}</h1>
          <p className="empty-message mt-2">
            Bank editing is unavailable in this build.
          </p>
        </div>
      </main>

      <aside
        className="map-pane min-w-0 p-3.5"
        aria-labelledby="map-list-title"
      >
        <h2 className="text-sm font-semibold" id="map-list-title">
          Map List
        </h2>
        <Separator className="my-2.5" isDecorative />
        <p className="empty-message text-sm">No maps loaded.</p>
      </aside>
    </div>
  );
}
