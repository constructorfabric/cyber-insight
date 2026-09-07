import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

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
});
