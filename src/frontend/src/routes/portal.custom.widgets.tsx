import { Link, createFileRoute } from "@tanstack/react-router";
import { useQuery } from "@tanstack/react-query";

import type { Widget } from "@/api/custom-client";
import {
  DefinitionCard,
  DefinitionList,
} from "@/components/custom/definition-list";
import { Badge } from "@/components/ui/badge";
import { CenteredSpinner } from "@/components/widgets/centered-spinner";
import { widgetNamesQuery, widgetQuery } from "@/queries/custom";
import { TEXT_BODY, TEXT_LABEL } from "@/lib/type-scale";
import { cn } from "@/lib/utils";

/** Static, so it wins over `$name` — see the note on the metrics route. */
export const Route = createFileRoute("/portal/custom/widgets")({
  component: WidgetsCatalogue,
});

function WidgetsCatalogue() {
  const { data: names, isLoading, isError, refetch } = useQuery(
    widgetNamesQuery()
  );

  return (
    <DefinitionList
      title="Widgets"
      blurb="Every stored visual. Each draws one metric; a dashboard holds them."
      names={names}
      isLoading={isLoading}
      isError={isError}
      onRetry={() => void refetch()}
      emptyLabel="No widgets yet. Ask the assistant for one."
      renderRow={(name) => <WidgetRow name={name} />}
    />
  );
}

function WidgetRow({ name }: { name: string }) {
  const { data, isPending, isError, error } = useQuery(widgetQuery(name));

  return (
    <DefinitionCard name={name} kind="widgets">
      {isPending ? (
        <CenteredSpinner className="min-h-24" />
      ) : isError ? (
        <p role="alert" className={cn(TEXT_BODY, "text-destructive")}>
          {(error as Error).message}
        </p>
      ) : (
        <WidgetSummary widget={data} />
      )}
    </DefinitionCard>
  );
}

function WidgetSummary({ widget }: { widget: Widget }) {
  return (
    <div className="flex flex-col gap-2">
      <Badge variant="secondary" className="w-fit font-mono">
        {widget.type}
      </Badge>
      <dl className="grid grid-cols-[auto_1fr] gap-x-3 gap-y-1.5">
        <dt className={TEXT_LABEL}>Metric</dt>
        <dd className={cn(TEXT_BODY, "min-w-0 break-words")}>
          {/* The metrics page is where its query is, so the name links there. */}
          <Link
            to="/portal/custom/metrics"
            className="font-mono underline decoration-dotted underline-offset-4"
          >
            {widget.metric}
          </Link>
        </dd>
        {widget.type === "table" ? (
          <>
            <dt className={TEXT_LABEL}>Columns</dt>
            <dd className={cn(TEXT_BODY, "min-w-0 break-words font-mono")}>
              {widget.columns.join(", ")}
            </dd>
          </>
        ) : (
          <>
            <dt className={TEXT_LABEL}>Axes</dt>
            <dd className={cn(TEXT_BODY, "min-w-0 break-words font-mono")}>
              x {widget.x} · y {widget.y}
            </dd>
          </>
        )}
      </dl>
    </div>
  );
}
