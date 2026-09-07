import type { ReactNode } from "react";

/**
 * Two columns inside the portal inset: the dashboards scroll on the left, the
 * assistant keeps a fixed column on the right. Below `lg` the panel drops under
 * the content instead of squeezing it — one chat instance either way, so the
 * thread survives the breakpoint.
 */
export function CustomPageShell({
  children,
  chat,
}: {
  children: ReactNode;
  chat: ReactNode;
}) {
  return (
    <div className="flex min-h-0 flex-1 flex-col lg:flex-row">
      <div className="@container min-w-0 flex-1 overflow-y-auto p-4 md:p-6">
        {children}
      </div>
      <div className="flex min-h-96 shrink-0 flex-col lg:min-h-0 lg:w-80">
        {chat}
      </div>
    </div>
  );
}
