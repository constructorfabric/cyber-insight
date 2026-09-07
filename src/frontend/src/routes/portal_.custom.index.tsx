import { createFileRoute, Link } from "@tanstack/react-router";
import { useQuery } from "@tanstack/react-query";

import { CenteredSpinner } from "@/components/widgets/centered-spinner";
import { ComingSoon } from "@/components/widgets/coming-soon";
import { dashboardNamesQuery } from "@/queries/custom";

export const Route = createFileRoute("/portal_/custom/")({
  component: CustomDashboardIndex,
});

function CustomDashboardIndex() {
  const {
    data: names,
    isLoading,
    isError,
    refetch,
  } = useQuery(dashboardNamesQuery());

  if (isLoading) return <CenteredSpinner className="min-h-40" />;
  if (isError) {
    return (
      <ComingSoon
        variant="card"
        state="error"
        label="Couldn't load the dashboard list."
        onRetry={() => void refetch()}
      />
    );
  }

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
