import {
  CartesianGrid,
  Line,
  LineChart,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from "recharts";

import type { MetricResult } from "@/api/custom-client";

export interface CustomLineChartProps {
  result: MetricResult;
  x: string;
  y: string;
}

export function CustomLineChart({ result, x, y }: CustomLineChartProps) {
  const xIndex = result.columns.indexOf(x);
  const yIndex = result.columns.indexOf(y);
  const data = result.rows.map((row) => ({
    [x]: row[xIndex],
    [y]: row[yIndex],
  }));

  return (
    <div data-testid="custom-line-chart" className="h-64 w-full">
      <ResponsiveContainer width="100%" height="100%">
        <LineChart data={data}>
          <CartesianGrid strokeDasharray="3 3" />
          <XAxis dataKey={x} />
          <YAxis />
          <Tooltip />
          <Line type="linear" dataKey={y} />
        </LineChart>
      </ResponsiveContainer>
    </div>
  );
}
