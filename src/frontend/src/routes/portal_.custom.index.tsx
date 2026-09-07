import { createFileRoute, Link, useNavigate } from "@tanstack/react-router";
import { useQuery, useQueryClient } from "@tanstack/react-query";

import type { ChatCreated } from "@/api/custom-client";
import { CustomChat } from "@/components/custom/custom-chat";
import { CenteredSpinner } from "@/components/widgets/centered-spinner";
import { ComingSoon } from "@/components/widgets/coming-soon";
import { dashboardNamesQuery, invalidateDashboardList } from "@/queries/custom";

export const Route = createFileRoute("/portal_/custom/")({
  component: CustomDashboardIndex,
});

function CustomDashboardIndex() {
  const queryClient = useQueryClient();
  const navigate = useNavigate();
  const {
    data: names,
    isLoading,
    isError,
    refetch,
  } = useQuery(dashboardNamesQuery());

  function handleCreated(created: ChatCreated) {
    void invalidateDashboardList(queryClient);
    if (created.dashboard) {
      void navigate({
        to: "/portal/custom/$name",
        params: { name: created.dashboard },
      });
    }
  }

  return (
    <div>
      <CustomChat onCreated={handleCreated} />
      <CustomDashboardList
        names={names}
        isLoading={isLoading}
        isError={isError}
        onRetry={() => void refetch()}
      />
    </div>
  );
}

function CustomDashboardList({
  names,
  isLoading,
  isError,
  onRetry,
}: {
  names: string[] | undefined;
  isLoading: boolean;
  isError: boolean;
  onRetry: () => void;
}) {
  if (isLoading) return <CenteredSpinner className="min-h-40" />;
  if (isError) {
    return (
      <ComingSoon
        variant="card"
        state="error"
        label="Couldn't load the dashboard list."
        onRetry={onRetry}
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
