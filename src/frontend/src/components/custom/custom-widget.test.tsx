import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

// Deterministic, DOM-inspectable stand-ins for the recharts wrappers (which
// render nothing under jsdom's zero-size ResponsiveContainer).
vi.mock("recharts", () => ({
  ResponsiveContainer: ({ children }: { children: React.ReactNode }) => (
    <>{children}</>
  ),
  LineChart: ({
    data,
    children,
  }: {
    data: Record<string, unknown>[];
    children: React.ReactNode;
  }) => (
    <div data-testid="line-chart" data-chart-data={JSON.stringify(data)}>
      {children}
    </div>
  ),
  CartesianGrid: () => null,
  Tooltip: () => null,
  XAxis: ({ dataKey }: { dataKey: string }) => (
    <div data-testid="x-axis" data-key={dataKey} />
  ),
  YAxis: () => null,
  Line: ({ dataKey }: { dataKey: string }) => (
    <div data-testid="line" data-key={dataKey} />
  ),
}));

import { CustomWidget } from "./custom-widget";

const result = {
  columns: ["day", "lines"],
  rows: [
    ["2026-09-01", 59],
    ["2026-09-02", 12],
  ],
};

describe("<CustomWidget>", () => {
  it("renders a table widget as rows", () => {
    render(
      <CustomWidget
        widget={{ type: "table", metric: "m", columns: ["day", "lines"] }}
        result={result}
      />,
    );

    expect(screen.getByRole("columnheader", { name: "day" })).toBeInTheDocument();
    expect(screen.getAllByRole("row")).toHaveLength(3);
  });

  it("renders a line widget as a chart", () => {
    render(
      <CustomWidget widget={{ type: "line", metric: "m", x: "day", y: "lines" }} result={result} />,
    );

    expect(screen.getByTestId("custom-line-chart")).toBeInTheDocument();
  });

  it("maps the widget's x and y fields to the chart axes and row values", () => {
    render(
      <CustomWidget widget={{ type: "line", metric: "m", x: "day", y: "lines" }} result={result} />,
    );

    expect(screen.getByTestId("x-axis")).toHaveAttribute("data-key", "day");
    expect(screen.getByTestId("line")).toHaveAttribute("data-key", "lines");

    const chartData = JSON.parse(
      screen.getByTestId("line-chart").getAttribute("data-chart-data")!,
    );
    expect(chartData).toEqual([
      { day: "2026-09-01", lines: 59 },
      { day: "2026-09-02", lines: 12 },
    ]);
  });

  it("shows an error in place of the content when the run failed", () => {
    render(
      <CustomWidget
        widget={{ type: "table", metric: "m", columns: ["day"] }}
        error={new Error("unknown table `evnts`")}
      />,
    );

    expect(screen.getByRole("alert")).toHaveTextContent("unknown table `evnts`");
  });

  it("says there is no data when the metric returned no rows", () => {
    render(
      <CustomWidget
        widget={{ type: "table", metric: "m", columns: ["day"] }}
        result={{ columns: ["day"], rows: [] }}
      />,
    );

    expect(screen.getByText(/no data/i)).toBeInTheDocument();
  });

  it("says so when the type is unknown", () => {
    render(<CustomWidget widget={{ type: "sankey", metric: "m" } as never} result={result} />);

    expect(screen.getByText(/unknown widget type/i)).toBeInTheDocument();
  });

  it("says there is no data when neither a result nor an error was passed", () => {
    render(<CustomWidget widget={{ type: "table", metric: "m", columns: ["day"] }} />);

    expect(screen.getByText(/no data/i)).toBeInTheDocument();
  });

  it("pads a short row with empty cells instead of misaligning columns", () => {
    render(
      <CustomWidget
        widget={{ type: "table", metric: "m", columns: ["day", "lines", "extra"] }}
        result={{ columns: ["day", "lines", "extra"], rows: [["2026-09-01", 59]] }}
      />,
    );

    const cells = screen.getAllByRole("cell");
    expect(cells).toHaveLength(3);
    expect(cells[0]).toHaveTextContent("2026-09-01");
    expect(cells[1]).toHaveTextContent("59");
    expect(cells[2]).toHaveTextContent("");
  });

  it("does not spill a long row past the declared columns", () => {
    render(
      <CustomWidget
        widget={{ type: "table", metric: "m", columns: ["day"] }}
        result={{ columns: ["day"], rows: [["2026-09-01", 59, "extra"]] }}
      />,
    );

    const cells = screen.getAllByRole("cell");
    expect(cells).toHaveLength(1);
    expect(cells[0]).toHaveTextContent("2026-09-01");
  });

  it("says which column is missing instead of drawing an empty chart", () => {
    // Seen live: y named the metric's raw json field instead of its as_name,
    // so every point was undefined and the chart drew axes and no line.
    render(
      <CustomWidget
        widget={{ type: "line", metric: "lines_per_day", x: "day", y: "lines" }}
        result={{
          columns: ["day", "total_lines"],
          rows: [["2026-09-01", 132]],
        }}
      />
    );

    const alert = screen.getByRole("alert");
    expect(alert).toHaveTextContent("draws lines");
    expect(alert).toHaveTextContent("lines_per_day does not return");
    expect(alert).toHaveTextContent("day, total_lines");
    expect(screen.queryByTestId("custom-line-chart")).not.toBeInTheDocument();
  });
});
