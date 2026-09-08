import { fetchWithAuth } from "@/api/fetch-with-auth";

const BASE =
  (import.meta.env.VITE_API_BASE_V3 as string | undefined) ?? "/api/v3/v1";

export interface TableWidget {
  type: "table";
  metric: string;
  columns: string[];
}

/** A line, a bar and an area all read one column against another. */
export interface SeriesWidget {
  type: "line" | "bar" | "area";
  metric: string;
  x: string;
  y: string;
}

export interface StatWidget {
  type: "stat";
  metric: string;
  value: string;
  label?: string;
}

export interface PieWidget {
  type: "pie";
  metric: string;
  label: string;
  value: string;
}

export type Widget = TableWidget | SeriesWidget | StatWidget | PieWidget;

/**
 * A metric's stored query, as the service interprets it.
 *
 * A field reads either a key inside an ingested payload (`json`) or a real
 * column of a table on the stand (`column`) — never both, and a lone `count`
 * needs neither.
 */
export interface MetricDefinition {
  database?: string;
  table: string;
  fields: {
    json?: string;
    column?: string;
    type: string;
    agg?: string;
    as_name: string;
    person?: "email" | "id";
  }[];
  group_by?: string[];
  filters?: {
    json?: string;
    column?: string;
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

/** One turn already in the thread, sent back so the model can read it. */
export interface ChatTurn {
  role: "user" | "assistant";
  content: string;
}

export interface ChatReply {
  reply: string;
  result?: MetricResult;
  created?: ChatCreated;
  /** Names that already existed and now hold something else. */
  updated?: ChatCreated;
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

/** What a definition is, in the API's path segments. */
export type DefinitionKind = "metrics" | "widgets" | "dashboards";

/**
 * Removes a definition.
 *
 * Refused while something still draws it — a widget's metric, a dashboard's
 * widget — and the reply names what does, which is what the caller is shown.
 */
export async function deleteDefinition(
  kind: DefinitionKind,
  name: string
): Promise<void> {
  const res = await fetchWithAuth(`${BASE}/${kind}/${encodeURIComponent(name)}`, {
    method: "DELETE",
  });
  if (!res.ok) {
    throw new CustomApiError(res.status, await res.json().catch(() => null));
  }
}

/** The new name, and what the service pointed at it. */
export interface Renamed {
  name: string;
  rewritten: string[];
}

/**
 * Renames a definition, and everything that named the old one.
 *
 * Refused when the new name is taken — a rename that silently replaced
 * another definition would lose it.
 */
export async function renameDefinition(
  kind: DefinitionKind,
  name: string,
  to: string
): Promise<Renamed> {
  const res = await fetchWithAuth(
    `${BASE}/${kind}/${encodeURIComponent(name)}/rename`,
    { method: "POST", headers: JSON_HEADERS, body: JSON.stringify({ to }) }
  );
  if (!res.ok) {
    throw new CustomApiError(res.status, await res.json().catch(() => null));
  }

  return (await res.json()) as Renamed;
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

export async function sendChat(
  message: string,
  history: ChatTurn[] = []
): Promise<ChatReply> {
  const res = await fetchWithAuth(`${BASE}/chat`, {
    method: "POST",
    headers: JSON_HEADERS,
    // The service keeps no session, so the thread travels with every turn.
    body: JSON.stringify({ message, history }),
  });
  return readJson<ChatReply>(res);
}
