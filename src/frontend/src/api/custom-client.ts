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
