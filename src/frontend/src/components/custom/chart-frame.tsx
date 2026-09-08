import type { ReactElement } from "react";

import {
  ChartContainer,
  type ChartConfig,
  ChartLegend,
  ChartLegendContent,
  ChartTooltip,
  ChartTooltipContent,
} from "@gears-frontx/ui-kit";

/**
 * The chrome every chart shares: the kit's container, its theming and its
 * tooltip.
 *
 * Each kind contributes only its series, so what a chart looks like is the
 * kit's business and not repeated per widget type.
 */
export function ChartFrame({
  config,
  testId,
  children,
}: {
  config: ChartConfig;
  testId: string;
  children: ReactElement;
}) {
  return (
    <div data-testid={testId} className="h-64 w-full">
      <ChartContainer config={config} className="h-full w-full">
        {children}
      </ChartContainer>
    </div>
  );
}

export { ChartLegend, ChartLegendContent, ChartTooltip, ChartTooltipContent };
