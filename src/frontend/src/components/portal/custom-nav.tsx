import { Link, useRouterState } from "@tanstack/react-router";
import { LayoutDashboard } from "lucide-react";
import { useQuery } from "@tanstack/react-query";

import {
  SidebarGroup,
  SidebarGroupContent,
  SidebarGroupLabel,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
} from "@/components/ui/sidebar";
import { Spinner } from "@/components/ui/spinner";
import { cn } from "@/lib/utils";
import { dashboardNamesQuery } from "@/queries/custom";
import { TEXT_LABEL } from "@/lib/type-scale";

/**
 * The custom zone's pane: one nav row per dashboard, read from the same query
 * the page reads, so a dashboard the chat just built appears here as soon as
 * the list is invalidated — no reload.
 */
export function CustomNav() {
  const pathname = useRouterState({ select: (s) => s.location.pathname });
  const { data: names, isPending, isError } = useQuery(dashboardNamesQuery());

  return (
    <SidebarGroup>
      <SidebarGroupLabel>Dashboards</SidebarGroupLabel>
      <SidebarGroupContent>
        {isPending ? (
          <div className="px-2 py-1.5">
            <Spinner className="size-4" />
          </div>
        ) : isError ? (
          <p className={cn(TEXT_LABEL, "px-2 py-1.5")}>
            Couldn&apos;t load dashboards.
          </p>
        ) : names && names.length > 0 ? (
          <SidebarMenu>
            {names.map((name) => (
              <SidebarMenuItem key={name}>
                <SidebarMenuButton
                  isActive={pathname === `/portal/custom/${name}`}
                  render={
                    <Link to="/portal/custom/$name" params={{ name }} />
                  }
                >
                  <LayoutDashboard />
                  <span>{name}</span>
                </SidebarMenuButton>
              </SidebarMenuItem>
            ))}
          </SidebarMenu>
        ) : (
          <p className={cn(TEXT_LABEL, "px-2 py-1.5")}>
            No dashboards yet. Ask the assistant to build one.
          </p>
        )}
      </SidebarGroupContent>
    </SidebarGroup>
  );
}
