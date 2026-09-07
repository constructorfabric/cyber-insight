import { queryOptions } from "@tanstack/react-query";

import {
  fetchDashboard,
  fetchDashboardNames,
  fetchWidget,
  runMetric,
} from "@/api/custom-client";

export function dashboardNamesQuery() {
  return queryOptions({
    queryKey: ["custom", "dashboard-names"],
    queryFn: () => fetchDashboardNames(),
  });
}

export function dashboardQuery(name: string) {
  return queryOptions({
    queryKey: ["custom", "dashboard", name],
    queryFn: () => fetchDashboard(name),
  });
}

export function widgetQuery(name: string) {
  return queryOptions({
    queryKey: ["custom", "widget", name],
    queryFn: () => fetchWidget(name),
  });
}

export function metricResultQuery(name: string) {
  return queryOptions({
    queryKey: ["custom", "metric-result", name],
    queryFn: () => runMetric(name),
  });
}
