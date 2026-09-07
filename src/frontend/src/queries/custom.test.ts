import { describe, expect, it, vi } from "vitest";

vi.mock("@/api/custom-client");

import * as customClient from "@/api/custom-client";

import {
  dashboardNamesQuery,
  dashboardQuery,
  metricResultQuery,
  widgetQuery,
} from "./custom";

describe("dashboardNamesQuery", () => {
  it("asks fetchDashboardNames for its data", async () => {
    vi.mocked(customClient.fetchDashboardNames).mockResolvedValue([
      "engineering",
    ]);

    const options = dashboardNamesQuery();

    await expect(options.queryFn?.(undefined as never)).resolves.toEqual([
      "engineering",
    ]);
    expect(customClient.fetchDashboardNames).toHaveBeenCalledWith();
  });
});

describe("dashboardQuery", () => {
  it("keys on the dashboard name and asks fetchDashboard", async () => {
    const dashboard = { title: "Engineering", widgets: ["commits_table"] };
    vi.mocked(customClient.fetchDashboard).mockResolvedValue(dashboard);

    const options = dashboardQuery("engineering");

    await expect(options.queryFn?.(undefined as never)).resolves.toEqual(
      dashboard,
    );
    expect(customClient.fetchDashboard).toHaveBeenCalledWith("engineering");
    expect(dashboardQuery("engineering").queryKey).not.toEqual(
      dashboardQuery("delivery").queryKey,
    );
  });
});

describe("widgetQuery", () => {
  it("keys on the widget name and asks fetchWidget", async () => {
    const widget = { type: "table" as const, metric: "m", columns: ["day"] };
    vi.mocked(customClient.fetchWidget).mockResolvedValue(widget);

    const options = widgetQuery("commits_table");

    await expect(options.queryFn?.(undefined as never)).resolves.toEqual(
      widget,
    );
    expect(customClient.fetchWidget).toHaveBeenCalledWith("commits_table");
  });
});

describe("metricResultQuery", () => {
  it("keys on the metric name and asks runMetric", async () => {
    const result = { columns: ["day"], rows: [["2026-09-01"]] };
    vi.mocked(customClient.runMetric).mockResolvedValue(result);

    const options = metricResultQuery("commits_per_day");

    await expect(options.queryFn?.(undefined as never)).resolves.toEqual(
      result,
    );
    expect(customClient.runMetric).toHaveBeenCalledWith("commits_per_day");
  });
});
