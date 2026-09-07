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

export interface MetricResult {
  columns: string[];
  rows: unknown[][];
}

export interface Dashboard {
  title: string;
  widgets: string[];
}

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
    `${BASE}/dashboards/${encodeURIComponent(name)}`,
  );
  return readJson<Dashboard>(res);
}

export async function fetchWidget(name: string): Promise<Widget> {
  const res = await fetchWithAuth(
    `${BASE}/widgets/${encodeURIComponent(name)}`,
  );
  return readJson<Widget>(res);
}

export async function runMetric(name: string): Promise<MetricResult> {
  const res = await fetchWithAuth(
    `${BASE}/metrics/${encodeURIComponent(name)}/run`,
    { method: "POST" },
  );
  return readJson<MetricResult>(res);
}
