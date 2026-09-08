import {
  queryOptions,
  useMutation,
  useQueryClient,
  type QueryClient,
} from "@tanstack/react-query";

import type { ChatTurn, DefinitionKind } from "@/api/custom-client";
import {
  deleteDefinition,
  fetchDashboard,
  fetchDashboardNames,
  fetchMetric,
  fetchMetricNames,
  fetchWidget,
  fetchWidgetNames,
  renameDefinition,
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

export function metricNamesQuery() {
  return queryOptions({
    queryKey: ["custom", "metric-names"],
    queryFn: () => fetchMetricNames(),
  });
}

export function metricQuery(name: string) {
  return queryOptions({
    queryKey: ["custom", "metric", name],
    queryFn: () => fetchMetric(name),
  });
}

export function widgetNamesQuery() {
  return queryOptions({
    queryKey: ["custom", "widget-names"],
    queryFn: () => fetchWidgetNames(),
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

export function useRemoveDefinition() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: ({ kind, name }: { kind: DefinitionKind; name: string }) =>
      deleteDefinition(kind, name),
    // Every catalogue and the pane read these lists.
    onSuccess: () => invalidateDashboardList(queryClient),
  });
}

export function useRenameDefinition() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: ({
      kind,
      name,
      to,
    }: {
      kind: DefinitionKind;
      name: string;
      to: string;
    }) => renameDefinition(kind, name, to),
    // A rename moves a body to a new key and rewrites its dependents, so
    // every cached definition is suspect, not just the lists.
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ["custom"] }),
  });
}

export function useSendChat() {
  return useMutation({
    mutationFn: ({ message, history }: { message: string; history: ChatTurn[] }) =>
      sendChat(message, history),
  });
}

export function invalidateDashboardList(queryClient: QueryClient) {
  return Promise.all([
    queryClient.invalidateQueries({ queryKey: dashboardNamesQuery().queryKey }),
    // The catalogue pages read these, and a chat that built a dashboard
    // built its metric and widgets too.
    queryClient.invalidateQueries({ queryKey: metricNamesQuery().queryKey }),
    queryClient.invalidateQueries({ queryKey: widgetNamesQuery().queryKey }),
  ]);
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
