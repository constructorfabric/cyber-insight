import { createFileRoute } from "@tanstack/react-router";
import { useQuery } from "@tanstack/react-query";

import type { MetricDefinition } from "@/api/custom-client";
import {
  DefinitionCard,
  DefinitionList,
} from "@/components/custom/definition-list";
import { CenteredSpinner } from "@/components/widgets/centered-spinner";
import { metricNamesQuery, metricQuery } from "@/queries/custom";
import { TEXT_BODY, TEXT_LABEL } from "@/lib/type-scale";
import { cn } from "@/lib/utils";

/**
 * A static segment, so it wins over `$name` in the route tree — a dashboard
 * literally called "metrics" would be unreachable, which is the trade for a
 * readable URL.
 */
export const Route = createFileRoute("/portal/custom/metrics")({
  component: MetricsCatalogue,
});

function MetricsCatalogue() {
  const { data: names, isLoading, isError, refetch } = useQuery(
    metricNamesQuery()
  );

  return (
    <DefinitionList
      title="Metrics"
      blurb="Every stored query. A widget draws one of these; the assistant can build more."
      names={names}
      isLoading={isLoading}
      isError={isError}
      onRetry={() => void refetch()}
      emptyLabel="No metrics yet. Ask the assistant for one."
      renderRow={(name) => <MetricRow name={name} />}
    />
  );
}

function MetricRow({ name }: { name: string }) {
  const { data, isPending, isError, error } = useQuery(metricQuery(name));

  return (
    <DefinitionCard name={name} kind="metrics">
      {isPending ? (
        <CenteredSpinner className="min-h-24" />
      ) : isError ? (
        <p role="alert" className={cn(TEXT_BODY, "text-destructive")}>
          {(error as Error).message}
        </p>
      ) : (
        <MetricSummary definition={data} />
      )}
    </DefinitionCard>
  );
}

function MetricSummary({ definition }: { definition: MetricDefinition }) {
  const grouped = definition.group_by ?? [];
  const filters = definition.filters ?? [];

  return (
    <dl className="grid grid-cols-[auto_1fr] gap-x-3 gap-y-1.5">
      <Row label="Table">
        <code className="font-mono">{definition.table}</code>
      </Row>
      <Row label="Fields">
        <span className="font-mono">
          {definition.fields
            .map((field) =>
              field.agg
                ? `${field.agg}(${field.json}) as ${field.as_name}`
                : `${field.json} as ${field.as_name}`
            )
            .join(", ")}
        </span>
      </Row>
      {grouped.length ? (
        <Row label="Grouped by">
          <span className="font-mono">{grouped.join(", ")}</span>
        </Row>
      ) : null}
      {filters.length ? (
        <Row label="Filtered">
          <span className="font-mono">
            {filters
              .map((f) => `${f.json} ${f.op} ${String(f.value)}`)
              .join(", ")}
          </span>
        </Row>
      ) : null}
      {definition.order_by ? (
        <Row label="Ordered by">
          <span className="font-mono">
            {definition.order_by.field} {definition.order_by.direction ?? "asc"}
          </span>
        </Row>
      ) : null}
      {definition.limit ? (
        <Row label="Limit">
          <span className="font-mono">{definition.limit}</span>
        </Row>
      ) : null}
    </dl>
  );
}

function Row({
  label,
  children,
}: {
  label: string;
  children: React.ReactNode;
}) {
  return (
    <>
      <dt className={TEXT_LABEL}>{label}</dt>
      <dd className={cn(TEXT_BODY, "min-w-0 break-words")}>{children}</dd>
    </>
  );
}
