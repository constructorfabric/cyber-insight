import { useState } from "react";

import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Label } from "@/components/ui/label";
import { CenteredSpinner } from "@/components/widgets/centered-spinner";
import { ComingSoon } from "@/components/widgets/coming-soon";
import { TEXT_BODY } from "@/lib/type-scale";
import { cn } from "@/lib/utils";
import {
  useSaveTimezone,
  useViewerTimezone,
} from "@/queries/identity-preferences";

function knownTimezones(): string[] {
  const supported = (
    Intl as typeof Intl & { supportedValuesOf?: (key: string) => string[] }
  ).supportedValuesOf;
  const zones = supported ? supported("timeZone") : [];

  return zones.includes("UTC") ? zones : ["UTC", ...zones];
}

const TIMEZONES = knownTimezones();

/**
 * The reader's own settings.
 *
 * Today that is one setting: the zone a custom dashboard's days, weeks and
 * months are cut on. It is stored per person and per tenant, so it follows
 * them to another device rather than living in this browser.
 */
export function ProfileView() {
  const saved = useViewerTimezone();
  const save = useSaveTimezone();
  const [picked, setPicked] = useState<string | null>(null);

  if (saved.isPending) return <CenteredSpinner className="min-h-40" />;
  if (saved.isError || !saved.timezone) {
    return (
      <div className="mx-auto w-full max-w-md p-8">
        <ComingSoon
          variant="card"
          state="error"
          label="Couldn't read your settings."
          onRetry={saved.retry}
        />
      </div>
    );
  }

  const current = picked ?? saved.timezone;
  const changed = current !== saved.timezone;

  return (
    <div className="mx-auto w-full max-w-xl p-4">
      <Card>
        <CardHeader>
          <CardTitle>Profile</CardTitle>
          <CardDescription>
            Your timezone decides where a day starts on every custom dashboard
            you open: the boundaries of Yesterday, of each daily bar, and of a
            calendar month.
          </CardDescription>
        </CardHeader>
        <CardContent className="flex flex-col gap-3">
          <div className={cn(TEXT_BODY, "flex flex-col gap-1")}>
            <Label htmlFor="profile-timezone" className="font-medium">
              Timezone
            </Label>
            <select
              id="profile-timezone"
              className="h-9 rounded-md border border-input bg-transparent px-2.5 text-sm shadow-xs outline-none focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50"
              value={current}
              onChange={(event) => setPicked(event.target.value)}
            >
              {TIMEZONES.map((zone) => (
                <option key={zone} value={zone}>
                  {zone}
                </option>
              ))}
            </select>
          </div>

          <div className="flex items-center gap-3">
            <Button
              size="sm"
              disabled={!changed || save.isPending}
              onClick={() => save.mutate(current)}
            >
              Save
            </Button>
            {save.isSuccess && save.data?.timezone === current ? (
              <span role="status" className="text-sm text-muted-foreground">
                Saved.
              </span>
            ) : null}
            {save.isError ? (
              <span role="alert" className="text-sm text-destructive">
                Couldn't save that timezone.
              </span>
            ) : null}
          </div>
        </CardContent>
      </Card>
    </div>
  );
}
