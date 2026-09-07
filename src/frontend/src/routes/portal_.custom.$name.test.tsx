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
});
