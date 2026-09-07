import type { MetricResult, Widget } from "@/api/custom-client";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { CustomLineChart } from "@/components/custom/custom-line-chart";
import { CustomTable } from "@/components/custom/custom-table";

export interface CustomWidgetProps {
  widget: Widget;
  result?: MetricResult;
  error?: Error;
}

export function CustomWidget({ widget, result, error }: CustomWidgetProps) {
  if (error) {
    return (
      <Alert variant="destructive">
        <AlertDescription>{error.message}</AlertDescription>
      </Alert>
    );
  }

  if (!result || result.rows.length === 0) {
    return <p>No data.</p>;
  }

  switch (widget.type) {
    case "table":
      return <CustomTable result={result} />;
    case "line":
      return <CustomLineChart result={result} x={widget.x} y={widget.y} />;
    default:
      return <p>Unknown widget type.</p>;
  }
}
