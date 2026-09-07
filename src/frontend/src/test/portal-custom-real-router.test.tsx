/**
 * `/portal/custom` and `/portal/custom/$name` are file routes nested under
 * `/portal` in the routes directory, but `PortalLayout` renders no
 * `<Outlet/>` — it renders zone content picked by a `?zone=` search param.
 * Every other test for these two routes renders the route's `component`
 * directly with `@tanstack/react-router` mocked out (see
 * `src/test/portal-router.tsx`), so none of them would have caught that:
 * the route matched, the component rendered, and the portal shell around it
 * was never in the picture.
 *
 * This one builds a router from the real generated `routeTree` and drives it
 * with a memory history, so the actual route-matching and parenting decide
 * what renders — the same tree `router.ts` hands to `RouterProvider` in the
 * app.
 */

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import {
  RouterProvider,
  createMemoryHistory,
  createRouter,
} from "@tanstack/react-router";
import { render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import "@/i18n";
import { authStore } from "@/auth/auth-store";
import { makeSession } from "@/test/session";
import { routeTree } from "@/routeTree.gen";
import * as customClient from "@/api/custom-client";

vi.mock("@/api/custom-client");

function renderAt(path: string) {
  const router = createRouter({
    routeTree,
    history: createMemoryHistory({ initialEntries: [path] }),
  });
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <QueryClientProvider client={queryClient}>
      <RouterProvider router={router} />
    </QueryClientProvider>,
  );
}

beforeEach(() => {
  vi.resetAllMocks();
  // Empty personId skips the root's viewer-identity prefetch, so this test
  // needs no `@/api/identity-client` mock.
  authStore.setAuthenticated(makeSession({ personId: "" }));
});

afterEach(() => {
  authStore.reset();
});

describe("the /portal/custom routes, through the real router", () => {
  it("renders the dashboard list at /portal/custom, not the portal shell", async () => {
    vi.mocked(customClient.fetchDashboardNames).mockResolvedValue([
      "engineering",
    ]);

    renderAt("/portal/custom");

    expect(
      await screen.findByRole("link", { name: "engineering" }),
    ).toHaveAttribute("href", "/portal/custom/engineering");
    expect(
      document.querySelector('[data-slot="sidebar-wrapper"]'),
    ).not.toBeInTheDocument();
  });

  it("renders a dashboard at /portal/custom/$name, not the portal shell", async () => {
    vi.mocked(customClient.fetchDashboard).mockResolvedValue({
      title: "Engineering",
      widgets: [],
    });

    renderAt("/portal/custom/engineering");

    expect(await screen.findByText("Engineering")).toBeInTheDocument();
    expect(
      document.querySelector('[data-slot="sidebar-wrapper"]'),
    ).not.toBeInTheDocument();
  });
});
