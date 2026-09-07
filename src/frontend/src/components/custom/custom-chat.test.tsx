vi.mock("@/api/custom-client");

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { createElement, type ReactNode } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import * as customClient from "@/api/custom-client";

import { CustomChat } from "./custom-chat";

function wrapper({ children }: { children: ReactNode }) {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return createElement(QueryClientProvider, { client: queryClient }, children);
}

beforeEach(() => {
  vi.resetAllMocks();
});

describe("<CustomChat>", () => {
  it("answers a one-time question with a table in the chat", async () => {
    vi.mocked(customClient.sendChat).mockResolvedValue({
      reply: "59 lines on 2026-09-01",
      result: { columns: ["day", "lines"], rows: [["2026-09-01", 59]] },
    });
    const onCreated = vi.fn();

    render(<CustomChat onCreated={onCreated} />, { wrapper });
    await userEvent.type(
      screen.getByRole("textbox"),
      "how many lines on the first?"
    );
    await userEvent.click(screen.getByRole("button", { name: /send/i }));

    expect(
      await screen.findByText("59 lines on 2026-09-01")
    ).toBeInTheDocument();
    expect(await screen.findByRole("cell", { name: "59" })).toBeInTheDocument();
    expect(onCreated).not.toHaveBeenCalled();
  });

  it("shows the reply and calls onCreated when a reply creates definitions", async () => {
    vi.mocked(customClient.sendChat).mockResolvedValue({
      reply: "Added a chart",
      created: { widgets: ["commits_graph"] },
    });
    const onCreated = vi.fn();

    render(<CustomChat onCreated={onCreated} />, { wrapper });
    await userEvent.type(screen.getByRole("textbox"), "chart commits per day");
    await userEvent.click(screen.getByRole("button", { name: /send/i }));

    expect(await screen.findByText("Added a chart")).toBeInTheDocument();
    expect(onCreated).toHaveBeenCalledWith({ widgets: ["commits_graph"] });
  });

  it("says a name was skipped because it already exists", async () => {
    vi.mocked(customClient.sendChat).mockResolvedValue({
      reply: "Made most of it",
      created: { widgets: [] },
      skipped: [{ kind: "widget", name: "commits_table", reason: "exists" }],
    });
    const onCreated = vi.fn();

    render(<CustomChat onCreated={onCreated} />, { wrapper });
    await userEvent.type(screen.getByRole("textbox"), "make commits_table");
    await userEvent.click(screen.getByRole("button", { name: /send/i }));

    expect(
      await screen.findByText("commits_table already exists, left as it was")
    ).toBeInTheDocument();
  });
});
