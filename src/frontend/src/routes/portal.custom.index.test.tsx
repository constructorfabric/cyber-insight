vi.mock("@tanstack/react-router", async () => {
  const { portalRouterMock } = await import("@/test/portal-router");
  return {
    ...portalRouterMock(),
    createFileRoute: () => (options: Record<string, unknown>) => options,
  };
});
vi.mock("@/api/custom-client");

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen } from "@testing-library/react";
import { createElement, type ReactNode } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import * as customClient from "@/api/custom-client";
import { portalRouter } from "@/test/portal-router";

import { Route } from "./portal.custom.index";

const Component = (Route as unknown as { component: () => React.ReactNode })
  .component;

function wrapper({ children }: { children: ReactNode }) {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return createElement(QueryClientProvider, { client: queryClient }, children);
}

beforeEach(() => {
  vi.resetAllMocks();
  portalRouter.reset();
});

describe("/portal/custom", () => {
  it("lists every dashboard as a link", async () => {
    vi.mocked(customClient.fetchDashboardNames).mockResolvedValue([
      "engineering",
      "delivery",
    ]);

    render(<Component />, { wrapper });

    expect(
      await screen.findByRole("link", { name: "engineering" })
    ).toHaveAttribute("href", "/portal/custom/engineering");
    expect(
      await screen.findByRole("link", { name: "delivery" })
    ).toBeInTheDocument();
  });

  it("says so when there are no dashboards yet", async () => {
    vi.mocked(customClient.fetchDashboardNames).mockResolvedValue([]);

    render(<Component />, { wrapper });

    expect(await screen.findByText(/no dashboards yet/i)).toBeInTheDocument();
  });

  it("shows a loading state before the list resolves", () => {
    vi.mocked(customClient.fetchDashboardNames).mockReturnValue(
      new Promise(() => {})
    );

    render(<Component />, { wrapper });

    expect(
      screen.getByRole("status", { name: /loading/i })
    ).toBeInTheDocument();
  });

  it("shows a retryable error state when the list fails to load", async () => {
    vi.mocked(customClient.fetchDashboardNames).mockRejectedValue(
      new Error("network down")
    );

    render(<Component />, { wrapper });

    expect(
      await screen.findByRole("button", { name: /retry/i })
    ).toBeInTheDocument();
  });
});
