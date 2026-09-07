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
      return (
        drawable(widget.metric, widget.columns, result) ?? (
          <CustomTable result={result} />
        )
      );
    case "line":
      return (
        drawable(widget.metric, [widget.x, widget.y], result) ?? (
          <CustomLineChart result={result} x={widget.x} y={widget.y} />
        )
      );
    default:
      return <p>Unknown widget type.</p>;
  }
}

/**
 * What to render instead of the widget when its metric does not return a
 * column it draws, and nothing when it does.
 *
 * The service refuses such a widget now, but one stored before it did — or a
 * metric edited to drop a column — still reaches here, and a chart missing
 * its y column drew its axes and no line. That read as missing data rather
 * than as a definition naming a column that is not there.
 */
function drawable(
  metric: string,
  drawn: string[],
  result: MetricResult
): React.ReactElement | null {
  const missing = drawn.filter((column) => !result.columns.includes(column));
  if (missing.length === 0) return null;

  return (
    <Alert variant="destructive">
      <AlertDescription>
        This widget draws {missing.join(", ")}, which {metric} does not return.
        Its columns are {result.columns.join(", ")}.
      </AlertDescription>
    </Alert>
  );
}
