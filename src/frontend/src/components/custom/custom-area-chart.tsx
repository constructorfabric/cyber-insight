import { Area, AreaChart, CartesianGrid, XAxis, YAxis } from "recharts";

import type { MetricResult } from "@/api/custom-client";
import { points, seriesConfig } from "@/components/custom/chart-data";
import {
  ChartFrame,
  ChartTooltip,
  ChartTooltipContent,
} from "@/components/custom/chart-frame";

export interface CustomAreaChartProps {
  result: MetricResult;
  x: string;
  y: string;
}

export function CustomAreaChart({ result, x, y }: CustomAreaChartProps) {
  return (
    <ChartFrame config={seriesConfig(y)} testId="custom-area-chart">
      <AreaChart data={points(result, [x, y])}>
        <CartesianGrid vertical={false} className="stroke-border" />
        <XAxis dataKey={x} tickLine={false} axisLine={false} />
        <YAxis tickLine={false} axisLine={false} />
        <ChartTooltip content={<ChartTooltipContent />} />
        <Area
          dataKey={y}
          type="linear"
          stroke={`var(--color-${y})`}
          fill={`var(--color-${y})`}
          fillOpacity={0.2}
        />
      </AreaChart>
    </ChartFrame>
  );
}
