import { fetchWithAuth } from "@/api/fetch-with-auth";

const BASE =
  (import.meta.env.VITE_API_BASE_V3 as string | undefined) ?? "/api/v3/v1";

export interface TableWidget {
  type: "table";
  metric: string;
  columns: string[];
}

export interface LineWidget {
  type: "line";
  metric: string;
  x: string;
  y: string;
}

export type Widget = TableWidget | LineWidget;

/** A metric's stored query, as the service interprets it. */
export interface MetricDefinition {
  table: string;
  fields: {
    json: string;
    type: string;
    agg?: string;
    as_name: string;
  }[];
  group_by?: string[];
  filters?: {
    json: string;
    type: string;
    op: string;
    value: unknown;
  }[];
  order_by?: { field: string; direction?: "asc" | "desc" };
  limit?: number;
}

export interface MetricResult {
  columns: string[];
  rows: unknown[][];
}

export interface Dashboard {
  title: string;
  widgets: string[];
}

export interface ChatCreated {
  metric?: string;
  widgets: string[];
  dashboard?: string;
}

export interface ChatSkipped {
  kind: string;
  name: string;
  reason: string;
}

export interface ChatReply {
  reply: string;
  result?: MetricResult;
  created?: ChatCreated;
  skipped?: ChatSkipped[];
}

const JSON_HEADERS = { "Content-Type": "application/json" };

export class CustomApiError extends Error {
  status: number;
  body: unknown;

  constructor(status: number, body: unknown) {
    super(`Custom API ${status}`);
    this.name = "CustomApiError";
    this.status = status;
    this.body = body;
  }
}

async function readJson<T>(res: Response): Promise<T> {
  if (!res.ok) {
    throw new CustomApiError(res.status, await res.json().catch(() => null));
  }
  return (await res.json()) as T;
}

export async function fetchDashboardNames(): Promise<string[]> {
  const res = await fetchWithAuth(`${BASE}/dashboards`);
  const body = await readJson<{ names: string[] }>(res);
  return body.names;
}

export async function fetchDashboard(name: string): Promise<Dashboard> {
  const res = await fetchWithAuth(
    `${BASE}/dashboards/${encodeURIComponent(name)}`
  );
  return readJson<Dashboard>(res);
}

export async function fetchMetricNames(): Promise<string[]> {
  const res = await fetchWithAuth(`${BASE}/metrics`);
  const body = await readJson<{ names: string[] }>(res);
  return body.names;
}

export async function fetchMetric(name: string): Promise<MetricDefinition> {
  const res = await fetchWithAuth(
    `${BASE}/metrics/${encodeURIComponent(name)}`
  );
  return readJson<MetricDefinition>(res);
}

export async function fetchWidgetNames(): Promise<string[]> {
  const res = await fetchWithAuth(`${BASE}/widgets`);
  const body = await readJson<{ names: string[] }>(res);
  return body.names;
}

export async function fetchWidget(name: string): Promise<Widget> {
  const res = await fetchWithAuth(
    `${BASE}/widgets/${encodeURIComponent(name)}`
  );
  return readJson<Widget>(res);
}

export async function runMetric(name: string): Promise<MetricResult> {
  const res = await fetchWithAuth(
    `${BASE}/metrics/${encodeURIComponent(name)}/run`,
    { method: "POST" }
  );
  return readJson<MetricResult>(res);
}

export async function sendChat(message: string): Promise<ChatReply> {
  const res = await fetchWithAuth(`${BASE}/chat`, {
    method: "POST",
    headers: JSON_HEADERS,
    body: JSON.stringify({ message }),
  });
  return readJson<ChatReply>(res);
}
