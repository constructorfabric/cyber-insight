import { createFileRoute, useRouterState } from "@tanstack/react-router";
import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Table2 } from "lucide-react";

import { Button } from "@/components/ui/button";

import { CustomApiError } from "@/api/custom-client";
import { CustomWidget } from "@/components/custom/custom-widget";
import { WidgetDrilldown } from "@/components/custom/widget-drilldown";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { CenteredSpinner } from "@/components/widgets/centered-spinner";
import { ComingSoon } from "@/components/widgets/coming-soon";
import {
  dashboardQuery,
  metricResultQuery,
  widgetQuery,
} from "@/queries/custom";
import {
  TEXT_BODY,
  TEXT_HEADING,
  TEXT_LABEL,
  TEXT_TITLE,
} from "@/lib/type-scale";
import { cn } from "@/lib/utils";

export const Route = createFileRoute("/portal/custom/$name")({
  component: CustomDashboardPage,
});

function dashboardNameFromPath(pathname: string): string {
  const match = pathname.match(/^\/portal\/custom\/([^/]+)/);
  return match ? decodeURIComponent(match[1]) : "";
}

function CustomDashboardPage() {
  const pathname = useRouterState({ select: (s) => s.location.pathname });
  const name = dashboardNameFromPath(pathname);
  const {
    data: dashboard,
    isLoading,
    isError,
    error,
    refetch,
  } = useQuery(dashboardQuery(name));

  return (
    <CustomDashboardBody
      dashboard={dashboard}
      isLoading={isLoading}
      isError={isError}
      error={error}
      name={name}
      onRetry={() => void refetch()}
    />
  );
}

function CustomDashboardBody({
  dashboard,
  isLoading,
  isError,
  error,
  name,
  onRetry,
}: {
  dashboard: { title: string; widgets: string[] } | undefined;
  isLoading: boolean;
  isError: boolean;
  error: Error | null;
  name: string;
  onRetry: () => void;
}) {
  if (isLoading) return <CenteredSpinner className="min-h-40" />;
  if (isError) {
    if (error instanceof CustomApiError && error.status === 404) {
      return (
        <ComingSoon
          variant="card"
          state="empty"
          label={`No dashboard named "${name}".`}
        />
      );
    }
    return (
      <ComingSoon
        variant="card"
        state="error"
        label="Couldn't load this dashboard."
        onRetry={onRetry}
      />
    );
  }
  if (!dashboard) return null;

  return (
    <>
      <header className="mb-4 flex flex-wrap items-baseline gap-x-3 gap-y-1">
        <h1 className={TEXT_TITLE}>{dashboard.title}</h1>
        <span className={cn(TEXT_LABEL, "font-mono")}>{name}</span>
      </header>
      {dashboard.widgets.length === 0 ? (
        <ComingSoon
          variant="card"
          state="empty"
          label="This dashboard holds no widgets yet."
        />
      ) : (
        <div className="grid items-start gap-4 @3xl:grid-cols-2">
          {dashboard.widgets.map((widgetName) => (
            <DashboardWidgetSlot key={widgetName} name={widgetName} />
          ))}
        </div>
      )}
    </>
  );
}

function DashboardWidgetSlot({ name }: { name: string }) {
  const [drilldown, setDrilldown] = useState(false);
  const widgetState = useQuery(widgetQuery(name));
  const metric = widgetState.data?.metric;
  const resultState = useQuery({
    ...metricResultQuery(metric ?? ""),
    enabled: Boolean(metric),
  });

  if (widgetState.isPending) {
    return (
      <Card>
        <CardContent>
          <CenteredSpinner className="min-h-40" />
        </CardContent>
      </Card>
    );
  }
  if (widgetState.isError) {
    return (
      <Card>
        <CardContent>
          <p role="alert" className={cn(TEXT_BODY, "text-destructive")}>
            {(widgetState.error as Error).message}
          </p>
        </CardContent>
      </Card>
    );
  }

  const heading = widgetState.data.title;

  return (
    <Card>
      <CardHeader className="flex flex-row items-start gap-2">
        <div className="min-w-0 flex-1">
          <CardTitle className={cn(TEXT_HEADING, heading ? "" : "font-mono")}>
            {heading ?? name}
          </CardTitle>
          {heading ? (
            <p className={cn(TEXT_LABEL, "truncate font-mono")}>{name}</p>
          ) : null}
        </div>
        <Button
          variant="ghost"
          size="icon-sm"
          aria-label={`Show the data behind ${heading ?? name}`}
          className="text-muted-foreground"
          onClick={() => setDrilldown(true)}
        >
          <Table2 />
        </Button>
      </CardHeader>
      <CardContent
        role="button"
        tabIndex={0}
        aria-label={`Show the data behind ${heading ?? name}`}
        className="max-h-72 cursor-pointer overflow-auto"
        onClick={() => setDrilldown(true)}
        onKeyDown={(event) => {
          if (event.key === "Enter" || event.key === " ") {
            event.preventDefault();
            setDrilldown(true);
          }
        }}
      >
        <CustomWidget
          widget={widgetState.data}
          result={resultState.data}
          error={resultState.error as Error | undefined}
        />
      </CardContent>
      <WidgetDrilldown
        widget={widgetState.data}
        label={heading ?? name}
        open={drilldown}
        onOpenChange={setDrilldown}
      />
    </Card>
  );
}
