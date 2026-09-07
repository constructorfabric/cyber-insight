vi.mock("@tanstack/react-router", async () => {
  const { portalRouterMock } = await import("@/test/portal-router");
  return {
    ...portalRouterMock(),
    createFileRoute: () => (options: Record<string, unknown>) => options,
  };
});
vi.mock("@/api/custom-client", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/api/custom-client")>();
  return {
    ...actual,
    fetchDashboard: vi.fn(),
    fetchWidget: vi.fn(),
    runMetric: vi.fn(),
  };
});

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen } from "@testing-library/react";
import { createElement, type ReactNode } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import * as customClient from "@/api/custom-client";
import { portalRouter } from "@/test/portal-router";

import { Route } from "./portal_.custom.$name";

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

describe("/portal/custom/$name", () => {
  it("renders a widget per name in the dashboard", async () => {
    vi.mocked(customClient.fetchDashboard).mockResolvedValue({
      title: "Engineering",
      widgets: ["commits_table"],
    });
    vi.mocked(customClient.fetchWidget).mockResolvedValue({
      type: "table",
      metric: "commits_per_day",
      columns: ["day", "lines"],
    });
    vi.mocked(customClient.runMetric).mockResolvedValue({
      columns: ["day", "lines"],
      rows: [["2026-09-01", 59]],
    });
    portalRouter.go("/portal/custom/engineering");

    render(<Component />, { wrapper });

    expect(await screen.findByText("Engineering")).toBeInTheDocument();
    expect(
      await screen.findByRole("cell", { name: "2026-09-01" }),
    ).toBeInTheDocument();
    expect(
      await screen.findByRole("cell", { name: "59" }),
    ).toBeInTheDocument();
  });

  it("shows a loading state before the dashboard resolves", () => {
    vi.mocked(customClient.fetchDashboard).mockReturnValue(
      new Promise(() => {}),
    );
    portalRouter.go("/portal/custom/engineering");

    render(<Component />, { wrapper });

    expect(
      screen.getByRole("status", { name: /loading/i }),
    ).toBeInTheDocument();
  });

  it("says an unknown dashboard name was not found, with no retry offered", async () => {
    vi.mocked(customClient.fetchDashboard).mockRejectedValue(
      new customClient.CustomApiError(404, { title: "not found" }),
    );
    portalRouter.go("/portal/custom/does-not-exist");

    render(<Component />, { wrapper });

    expect(
      await screen.findByText(/no dashboard named/i),
    ).toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: /retry/i }),
    ).not.toBeInTheDocument();
  });

  it("shows a retryable error state when the dashboard fails to load for another reason", async () => {
    vi.mocked(customClient.fetchDashboard).mockRejectedValue(
      new Error("network down"),
    );
    portalRouter.go("/portal/custom/engineering");

    render(<Component />, { wrapper });

    expect(
      await screen.findByRole("button", { name: /retry/i }),
    ).toBeInTheDocument();
  });
});
