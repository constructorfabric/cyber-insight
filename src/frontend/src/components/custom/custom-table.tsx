import type { MetricResult } from "@/api/custom-client";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";

export interface CustomTableProps {
  result: MetricResult;
}

export function CustomTable({ result }: CustomTableProps) {
  return (
    <Table>
      <TableHeader>
        <TableRow>
          {result.columns.map((column) => (
            <TableHead key={column}>{column}</TableHead>
          ))}
        </TableRow>
      </TableHeader>
      <TableBody>
        {result.rows.map((row, rowIndex) => (
          <TableRow key={rowIndex}>
            {result.columns.map((_, columnIndex) => (
              <TableCell key={columnIndex}>
                {columnIndex < row.length ? String(row[columnIndex]) : ""}
              </TableCell>
            ))}
          </TableRow>
        ))}
      </TableBody>
    </Table>
  );
}
