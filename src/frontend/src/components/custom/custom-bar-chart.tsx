import { Bar, BarChart, CartesianGrid, XAxis, YAxis } from "recharts";

import type { MetricResult } from "@/api/custom-client";
import { points, seriesConfig } from "@/components/custom/chart-data";
import {
  ChartFrame,
  ChartTooltip,
  ChartTooltipContent,
} from "@/components/custom/chart-frame";

export interface CustomBarChartProps {
  result: MetricResult;
  x: string;
  y: string;
}

/** A count per category — the shape most questions about "per" answer with. */
export function CustomBarChart({ result, x, y }: CustomBarChartProps) {
  return (
    <ChartFrame config={seriesConfig(y)} testId="custom-bar-chart">
      <BarChart data={points(result, [x, y])}>
        <CartesianGrid vertical={false} className="stroke-border" />
        <XAxis dataKey={x} tickLine={false} axisLine={false} />
        <YAxis tickLine={false} axisLine={false} />
        <ChartTooltip content={<ChartTooltipContent />} />
        <Bar dataKey={y} fill={`var(--color-${y})`} radius={4} />
      </BarChart>
    </ChartFrame>
  );
}
