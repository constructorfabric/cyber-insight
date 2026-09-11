/**
 * The viewer's own settings (`GET/PUT /api/identity/v1/me/preferences`).
 *
 * Keyed by the session scope, like every other per-caller read: a sign-out and
 * sign-in as somebody else must not serve the previous caller's zone, which
 * would silently cut their days in the wrong place.
 */
import {
  useMutation,
  useQuery,
  useQueryClient,
  type UseQueryResult,
} from "@tanstack/react-query";

import {
  getPreferences,
  saveTimezone,
  type Preferences,
} from "@/api/identity-client";
import { useAuth } from "@/auth/use-auth";
import { sessionAuthorizationScope } from "@/auth/session-scope";

/** A setting changes when its owner changes it, and not otherwise. */
const PREFERENCES_STALE_TIME = 5 * 60 * 1000;

function usePreferences(): UseQueryResult<Preferences> {
  const { session } = useAuth();
  const sessionScope = sessionAuthorizationScope(session);

  return useQuery({
    queryKey: ["identity", "preferences", sessionScope],
    queryFn: getPreferences,
    staleTime: PREFERENCES_STALE_TIME,
    enabled: sessionScope != null,
  });
}

export interface ViewerTimezone {
  /** The zone to send with a windowed read, once it is known. */
  timezone: string | undefined;
  isPending: boolean;
  /** The read FAILED — which is not the same as "they chose UTC". */
  isError: boolean;
  retry: () => void;
}

/**
 * The zone the viewer reads dashboards in.
 *
 * A failure is reported rather than defaulted: showing UTC numbers to someone
 * who set Tokyo is a wrong answer with no sign that it is one.
 */
export function useViewerTimezone(): ViewerTimezone {
  const preferences = usePreferences();

  return {
    timezone: preferences.data?.timezone,
    isPending: preferences.isPending,
    isError: preferences.isError,
    retry: () => void preferences.refetch(),
  };
}

export function useSaveTimezone() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (timezone: string) => saveTimezone(timezone),
    onSuccess: () => {
      // Every windowed read is keyed by the zone.
      void queryClient.invalidateQueries({ queryKey: ["identity", "preferences"] });
      void queryClient.invalidateQueries({ queryKey: ["custom", "metric-result"] });
    },
  });
}
