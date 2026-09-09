import { createFileRoute } from "@tanstack/react-router";
import { useQuery } from "@tanstack/react-query";

import {
  DefinitionCard,
  DefinitionList,
} from "@/components/custom/definition-list";
import { MetricSummary } from "@/components/custom/definition-summary";
import { CenteredSpinner } from "@/components/widgets/centered-spinner";
import { metricNamesQuery, metricQuery } from "@/queries/custom";
import { TEXT_BODY } from "@/lib/type-scale";
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
