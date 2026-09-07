import { createFileRoute, Link } from "@tanstack/react-router";
import { useQuery } from "@tanstack/react-query";
import { ChevronRight, LayoutDashboard } from "lucide-react";

import { Card, CardContent } from "@/components/ui/card";
import { CenteredSpinner } from "@/components/widgets/centered-spinner";
import { ComingSoon } from "@/components/widgets/coming-soon";
import { dashboardNamesQuery } from "@/queries/custom";
import { TEXT_BODY, TEXT_NAME, TEXT_TITLE } from "@/lib/type-scale";
import { cn } from "@/lib/utils";

export const Route = createFileRoute("/portal/custom/")({
  component: CustomDashboardIndex,
});

function CustomDashboardIndex() {
  const {
    data: names,
    isLoading,
    isError,
    refetch,
  } = useQuery(dashboardNamesQuery());

  return (
    <>
      <header className="mb-4">
        <h1 className={TEXT_TITLE}>Custom</h1>
        <p className={cn(TEXT_BODY, "text-muted-foreground")}>
          Dashboards built from your own data. Ask the assistant for a new one.
        </p>
      </header>
      <CustomDashboardList
        names={names}
        isLoading={isLoading}
        isError={isError}
        onRetry={() => void refetch()}
      />
    </>
  );
}

function CustomDashboardList({
  names,
  isLoading,
  isError,
  onRetry,
}: {
  names: string[] | undefined;
  isLoading: boolean;
  isError: boolean;
  onRetry: () => void;
}) {
  if (isLoading) return <CenteredSpinner className="min-h-40" />;
  if (isError) {
    return (
      <ComingSoon
        variant="card"
        state="error"
        label="Couldn't load the dashboard list."
        onRetry={onRetry}
      />
    );
  }

  if (!names) return null;

  if (names.length === 0) {
    return (
      <ComingSoon
        variant="card"
        state="empty"
        label="No dashboards yet. Describe one to the assistant and it will build it."
      />
    );
  }

  return (
    <ul className="grid gap-3 @xl:grid-cols-2 @5xl:grid-cols-3">
      {names.map((name) => (
        <li key={name}>
          <Card
            size="sm"
            render={
              <Link
                to="/portal/custom/$name"
                params={{ name }}
                className="block transition-colors hover:bg-accent/50"
              />
            }
          >
            <CardContent className="flex items-center gap-3">
              <LayoutDashboard
                className="size-4 shrink-0 text-muted-foreground"
                aria-hidden
              />
              <span className={cn(TEXT_NAME, "min-w-0 truncate")}>{name}</span>
              <ChevronRight
                className="ms-auto size-4 shrink-0 text-muted-foreground"
                aria-hidden
              />
            </CardContent>
          </Card>
        </li>
      ))}
    </ul>
  );
}
