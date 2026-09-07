import {
  queryOptions,
  useMutation,
  type QueryClient,
} from "@tanstack/react-query";

import {
  fetchDashboard,
  fetchDashboardNames,
  fetchWidget,
  runMetric,
  sendChat,
} from "@/api/custom-client";

const WIDGET_QUERY_PREFIX = ["custom", "widget"] as const;

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
    queryKey: [...WIDGET_QUERY_PREFIX, name],
    queryFn: () => fetchWidget(name),
  });
}

export function metricResultQuery(name: string) {
  return queryOptions({
    queryKey: ["custom", "metric-result", name],
    queryFn: () => runMetric(name),
  });
}

export function useSendChat() {
  return useMutation({
    mutationFn: (message: string) => sendChat(message),
  });
}

export function invalidateDashboardList(queryClient: QueryClient) {
  return queryClient.invalidateQueries({
    queryKey: dashboardNamesQuery().queryKey,
  });
}

export function invalidateDashboardPage(
  queryClient: QueryClient,
  name: string
) {
  return Promise.all([
    queryClient.invalidateQueries({ queryKey: dashboardQuery(name).queryKey }),
    queryClient.invalidateQueries({ queryKey: WIDGET_QUERY_PREFIX }),
  ]);
}
