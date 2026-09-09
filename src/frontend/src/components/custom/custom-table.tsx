import type { MetricResult } from "@/api/custom-client";
import { groupedNumber } from "@/components/custom/chart-format";
import { TEXT_LABEL } from "@/lib/type-scale";
import { cn } from "@/lib/utils";
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

/** Beyond this the card is a scroll bar, and the DOM pays for every row. */
const SHOWN = 200;

export function CustomTable({ result }: CustomTableProps) {
  const rows = result.rows.slice(0, SHOWN);

  return (
    <>
      {result.rows.length > SHOWN ? (
        <p className={cn(TEXT_LABEL, "mb-2")}>
          First {groupedNumber(SHOWN)} of {groupedNumber(result.rows.length)}{" "}
          rows
        </p>
      ) : null}
      <Table>
        <TableHeader>
          <TableRow>
            {result.columns.map((column) => (
              <TableHead key={column}>{column}</TableHead>
            ))}
          </TableRow>
        </TableHeader>
        <TableBody>
          {rows.map((row, rowIndex) => (
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
    </>
  );
}
