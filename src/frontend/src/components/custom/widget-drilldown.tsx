import { useQuery } from "@tanstack/react-query";

import type { Widget } from "@/api/custom-client";
import { CustomTable } from "@/components/custom/custom-table";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { CenteredSpinner } from "@/components/widgets/centered-spinner";
import { metricQuery, metricResultQuery } from "@/queries/custom";
import { TEXT_BODY, TEXT_LABEL } from "@/lib/type-scale";
import { cn } from "@/lib/utils";

/**
 * The rows behind a widget.
 *
 * A chart is a shape; the question it prompts is "which rows are those?".
 * This opens the metric's own result — every row it returns, not the picture —
 * beside the table it came from. Controlled, because the card opens it from
 * two places: its own body, and the button in its header.
 */
export function WidgetDrilldown({
  widget,
  label,
  open,
  onOpenChange,
}: {
  widget: Widget;
  label: string;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-3xl">
        <DialogHeader>
          <DialogTitle>{label}</DialogTitle>
          <DialogDescription>
            Every row metric <span className="font-mono">{widget.metric}</span>{" "}
            returns.
          </DialogDescription>
        </DialogHeader>
        {open ? <Rows metric={widget.metric} /> : null}
      </DialogContent>
    </Dialog>
  );
}

function Rows({ metric }: { metric: string }) {
  const definition = useQuery(metricQuery(metric));
  const result = useQuery(metricResultQuery(metric));

  if (result.isPending) return <CenteredSpinner className="min-h-40" />;
  if (result.isError) {
    return (
      <p role="alert" className={cn(TEXT_BODY, "text-destructive")}>
        {(result.error as Error).message}
      </p>
    );
  }
  if (!result.data || result.data.rows.length === 0) {
    return <p className={TEXT_BODY}>No data.</p>;
  }

  const table = definition.data
    ? [definition.data.database, definition.data.table]
        .filter(Boolean)
        .join(".")
    : null;

  return (
    <div className="flex min-w-0 flex-col gap-2">
      {table ? <p className={cn(TEXT_LABEL, "font-mono")}>{table}</p> : null}
      <div className="max-h-[60vh] min-w-0 overflow-auto">
        <CustomTable result={result.data} />
      </div>
    </div>
  );
}
