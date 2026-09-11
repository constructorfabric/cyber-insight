vi.mock("@/api/identity-client", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/api/identity-client")>();
  return { ...actual, getPreferences: vi.fn(), saveTimezone: vi.fn() };
});
vi.mock("@/auth/use-auth", () => ({
  useAuth: () => ({
    session: {
      tenantId: "t1",
      personId: "p1",
      impersonatorEmail: null,
      roles: [],
    },
  }),
}));

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { createElement, type ReactNode } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import * as identityClient from "@/api/identity-client";

import { ProfileView } from "./profile-view";

function wrapper({ children }: { children: ReactNode }) {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return createElement(QueryClientProvider, { client: queryClient }, children);
}

beforeEach(() => {
  vi.resetAllMocks();
});

describe("<ProfileView>", () => {
  it("shows the zone that is saved, and what it decides", async () => {
    vi.mocked(identityClient.getPreferences).mockResolvedValue({
      timezone: "Europe/Belgrade",
    });

    render(<ProfileView />, { wrapper });

    expect(await screen.findByDisplayValue("Europe/Belgrade")).toBeVisible();
    expect(screen.getByText(/day.*(start|boundar)/i)).toBeVisible();
  });

  it("saves the zone the reader picked", async () => {
    vi.mocked(identityClient.getPreferences).mockResolvedValue({
      timezone: "UTC",
    });
    vi.mocked(identityClient.saveTimezone).mockResolvedValue({
      timezone: "Asia/Tokyo",
    });
    render(<ProfileView />, { wrapper });
    await screen.findByDisplayValue("UTC");

    await userEvent.selectOptions(
      screen.getByLabelText(/timezone/i),
      "Asia/Tokyo",
    );
    await userEvent.click(screen.getByRole("button", { name: /save/i }));

    expect(identityClient.saveTimezone).toHaveBeenCalledWith("Asia/Tokyo");
    expect(await screen.findByText(/saved/i)).toBeVisible();
  });

  it("has nothing to save until something changes", async () => {
    vi.mocked(identityClient.getPreferences).mockResolvedValue({
      timezone: "UTC",
    });

    render(<ProfileView />, { wrapper });
    await screen.findByDisplayValue("UTC");

    expect(screen.getByRole("button", { name: /save/i })).toBeDisabled();
  });

  it("says a save failed rather than looking like it worked", async () => {
    vi.mocked(identityClient.getPreferences).mockResolvedValue({
      timezone: "UTC",
    });
    vi.mocked(identityClient.saveTimezone).mockRejectedValue(
      new Error("refused"),
    );
    render(<ProfileView />, { wrapper });
    await screen.findByDisplayValue("UTC");

    await userEvent.selectOptions(
      screen.getByLabelText(/timezone/i),
      "Asia/Tokyo",
    );
    await userEvent.click(screen.getByRole("button", { name: /save/i }));

    expect(await screen.findByRole("alert")).toHaveTextContent(/couldn.t save/i);
  });

  it("offers a retry when the setting cannot be read at all", async () => {
    vi.mocked(identityClient.getPreferences).mockRejectedValue(
      new Error("down"),
    );

    render(<ProfileView />, { wrapper });

    expect(
      await screen.findByRole("button", { name: /retry/i }),
    ).toBeVisible();
  });
});
