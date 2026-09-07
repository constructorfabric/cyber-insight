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

import { Route } from "./portal.custom.metrics";

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

describe("/portal/custom/metrics", () => {
  it("names every metric and reads its query back", async () => {
    vi.mocked(customClient.fetchMetricNames).mockResolvedValue([
      "lines_per_day",
    ]);
    vi.mocked(customClient.fetchMetric).mockResolvedValue({
      table: "events",
      fields: [
        { json: "day", type: "string", as_name: "day" },
        { json: "lines", type: "int", agg: "sum", as_name: "total_lines" },
      ],
      group_by: ["day"],
      filters: [{ json: "event", type: "string", op: "eq", value: "commit" }],
      limit: 100,
    });

    render(<Component />, { wrapper });

    expect(await screen.findByText("lines_per_day")).toBeInTheDocument();
    expect(await screen.findByText("events")).toBeInTheDocument();
    // The aggregate and its alias, so a reader can tell which column is which.
    expect(
      await screen.findByText("day as day, sum(lines) as total_lines")
    ).toBeInTheDocument();
    expect(await screen.findByText("event eq commit")).toBeInTheDocument();
    expect(await screen.findByText("100")).toBeInTheDocument();
  });

  it("leaves out the rows a metric does not use", async () => {
    vi.mocked(customClient.fetchMetricNames).mockResolvedValue(["everything"]);
    vi.mocked(customClient.fetchMetric).mockResolvedValue({
      table: "events",
      fields: [{ json: "lines", type: "int", as_name: "lines" }],
      group_by: [],
      filters: [],
    });

    render(<Component />, { wrapper });

    expect(await screen.findByText("everything")).toBeInTheDocument();
    expect(screen.queryByText("Grouped by")).not.toBeInTheDocument();
    expect(screen.queryByText("Filtered")).not.toBeInTheDocument();
    expect(screen.queryByText("Limit")).not.toBeInTheDocument();
  });

  it("says so when there are no metrics yet", async () => {
    vi.mocked(customClient.fetchMetricNames).mockResolvedValue([]);

    render(<Component />, { wrapper });

    expect(await screen.findByText(/no metrics yet/i)).toBeInTheDocument();
  });

  it("shows a retryable error when the list fails to load", async () => {
    vi.mocked(customClient.fetchMetricNames).mockRejectedValue(
      new Error("network down")
    );

    render(<Component />, { wrapper });

    expect(
      await screen.findByRole("button", { name: /retry/i })
    ).toBeInTheDocument();
  });

  it("keeps the rest of the page when one metric fails to load", async () => {
    vi.mocked(customClient.fetchMetricNames).mockResolvedValue([
      "broken",
      "fine",
    ]);
    vi.mocked(customClient.fetchMetric).mockImplementation(async (name) => {
      if (name === "broken") throw new Error("that metric is gone");
      return {
        table: "events",
        fields: [{ json: "lines", type: "int", as_name: "lines" }],
      };
    });

    render(<Component />, { wrapper });

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "that metric is gone"
    );
    expect(await screen.findByText("fine")).toBeInTheDocument();
  });
});
