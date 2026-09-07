import { createFileRoute, useRouterState } from "@tanstack/react-router";
import { useQuery } from "@tanstack/react-query";

import { CustomWidget } from "@/components/custom/custom-widget";
import {
  dashboardQuery,
  metricResultQuery,
  widgetQuery,
} from "@/queries/custom";

export const Route = createFileRoute("/portal_/custom/$name")({
  component: CustomDashboardPage,
});

function dashboardNameFromPath(pathname: string): string {
  const match = pathname.match(/^\/portal\/custom\/([^/]+)/);
  return match ? decodeURIComponent(match[1]) : "";
}

function CustomDashboardPage() {
  const pathname = useRouterState({ select: (s) => s.location.pathname });
  const name = dashboardNameFromPath(pathname);
  const { data: dashboard, isPending } = useQuery(dashboardQuery(name));

  if (isPending || !dashboard) return null;

  return (
    <div>
      <h1>{dashboard.title}</h1>
      {dashboard.widgets.map((widgetName) => (
        <DashboardWidgetSlot key={widgetName} name={widgetName} />
      ))}
    </div>
  );
}

function DashboardWidgetSlot({ name }: { name: string }) {
  const widgetState = useQuery(widgetQuery(name));
  const metric = widgetState.data?.metric;
  const resultState = useQuery({
    ...metricResultQuery(metric ?? ""),
    enabled: Boolean(metric),
  });

  if (widgetState.isPending) return null;
  if (widgetState.isError) {
    return <p role="alert">{(widgetState.error as Error).message}</p>;
  }

  return (
    <CustomWidget
      widget={widgetState.data}
      result={resultState.data}
      error={resultState.error as Error | undefined}
    />
  );
}
