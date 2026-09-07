import { createFileRoute, Link } from "@tanstack/react-router";
import { useQuery } from "@tanstack/react-query";

import { dashboardNamesQuery } from "@/queries/custom";

export const Route = createFileRoute("/portal/custom/")({
  component: CustomDashboardIndex,
});

function CustomDashboardIndex() {
  const { data: names } = useQuery(dashboardNamesQuery());

  if (!names) return null;

  if (names.length === 0) {
    return <p>No dashboards yet.</p>;
  }

  return (
    <ul>
      {names.map((name) => (
        <li key={name}>
          <Link to="/portal/custom/$name" params={{ name }}>
            {name}
          </Link>
        </li>
      ))}
    </ul>
  );
}
