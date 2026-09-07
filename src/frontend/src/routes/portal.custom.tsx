import {
  Outlet,
  createFileRoute,
  useNavigate,
  useRouterState,
} from "@tanstack/react-router";
import { useQueryClient } from "@tanstack/react-query";

import type { ChatCreated } from "@/api/custom-client";
import { CustomChat } from "@/components/custom/custom-chat";
import { CustomPageShell } from "@/components/custom/custom-page-shell";
import {
  invalidateDashboardList,
  invalidateDashboardPage,
} from "@/queries/custom";

/**
 * The custom zone's layout: one assistant for the whole zone, beside whichever
 * page is open.
 *
 * The chat lives here rather than in each page because creating a dashboard
 * navigates to it — mounted per page, the panel unmounted mid-request and took
 * the conversation with it, so the reader lost what they had just asked.
 */
export const Route = createFileRoute("/portal/custom")({
  component: CustomZone,
});

function dashboardNameFromPath(pathname: string): string {
  const match = pathname.match(/^\/portal\/custom\/([^/]+)/);
  return match ? decodeURIComponent(match[1]) : "";
}

function CustomZone() {
  const pathname = useRouterState({ select: (s) => s.location.pathname });
  const openDashboard = dashboardNameFromPath(pathname);
  const queryClient = useQueryClient();
  const navigate = useNavigate();

  function handleCreated(created: ChatCreated) {
    void invalidateDashboardList(queryClient);
    // The page on screen may have just gained a widget rather than a whole
    // dashboard, so it is refreshed either way.
    if (openDashboard) {
      void invalidateDashboardPage(queryClient, openDashboard);
    }
    if (created.dashboard) {
      void invalidateDashboardPage(queryClient, created.dashboard);
      void navigate({
        to: "/portal/custom/$name",
        params: { name: created.dashboard },
      });
    }
  }

  return (
    <CustomPageShell chat={<CustomChat onCreated={handleCreated} />}>
      <Outlet />
    </CustomPageShell>
  );
}
