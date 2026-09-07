import { useEffect, useRef, useState } from "react";
import { Sparkles } from "lucide-react";

import { cn } from "@/lib/utils";

import type { ChatCreated, ChatReply } from "@/api/custom-client";
import { CustomTable } from "@/components/custom/custom-table";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Spinner } from "@/components/ui/spinner";
import { Textarea } from "@/components/ui/textarea";
import { useSendChat } from "@/queries/custom";
import { TEXT_BODY, TEXT_HEADING, TEXT_LABEL } from "@/lib/type-scale";

export interface CustomChatProps {
  onCreated: (created: ChatCreated) => void;
}

interface Exchange {
  id: number;
  question: string;
  reply?: ChatReply;
  error?: string;
}

/**
 * Which tool the model called, read from the shape of what came back: rows
 * mean it answered, names mean it built. Never inferred from the prose.
 */
function toolUsed(reply: ChatReply): string | null {
  if (reply.result) return "answer";
  if (reply.created || reply.skipped?.length) return "create";
  return null;
}

export function CustomChat({ onCreated }: CustomChatProps) {
  const [message, setMessage] = useState("");
  const [exchanges, setExchanges] = useState<Exchange[]>([]);
  const sendChat = useSendChat();
  const threadEnd = useRef<HTMLDivElement | null>(null);

  // A thread that grows below the fold reads as a dead panel.
  useEffect(() => {
    // Optional call: jsdom does not implement it, and a thread that cannot
    // scroll is still a thread.
    threadEnd.current?.scrollIntoView?.({ block: "end" });
  }, [exchanges]);

  async function handleSend() {
    const question = message.trim();
    if (!question) return;

    const id = Date.now() + Math.random();
    setMessage("");
    setExchanges((prev) => [...prev, { id, question }]);

    try {
      const reply = await sendChat.mutateAsync(question);
      setExchanges((prev) =>
        prev.map((exchange) =>
          exchange.id === id ? { ...exchange, reply } : exchange
        )
      );
      if (reply.created) onCreated(reply.created);
    } catch {
      setMessage(question);
      setExchanges((prev) =>
        prev.map((exchange) =>
          exchange.id === id
            ? { ...exchange, error: "The chat request failed." }
            : exchange
        )
      );
    }
  }

  return (
    <aside className="flex h-full min-h-0 flex-col border-s bg-sidebar">
      <header className="flex items-center gap-2 border-b px-4 py-3">
        <Sparkles className="size-4 text-muted-foreground" aria-hidden />
        <h2 className={TEXT_HEADING}>Assistant</h2>
      </header>

      <div className="min-h-0 flex-1 overflow-y-auto px-4 py-3">
        {exchanges.length === 0 ? (
          <p className={cn(TEXT_LABEL, "leading-relaxed")}>
            Ask a question about the data, or describe a dashboard to build.
          </p>
        ) : (
          <ul className="flex flex-col gap-4">
            {exchanges.map((exchange) => (
              <li key={exchange.id} className="flex flex-col gap-2">
                <p
                  className={cn(
                    TEXT_BODY,
                    "ms-auto max-w-[85%] rounded-2xl rounded-ee-sm bg-primary px-3 py-2 text-primary-foreground"
                  )}
                >
                  {exchange.question}
                </p>

                {exchange.reply ? (
                  <ChatAnswer reply={exchange.reply} />
                ) : exchange.error ? (
                  <p
                    role="alert"
                    className={cn(
                      TEXT_BODY,
                      "me-auto max-w-[85%] rounded-2xl rounded-es-sm bg-destructive/10 px-3 py-2 text-destructive"
                    )}
                  >
                    {exchange.error}
                  </p>
                ) : (
                  <span
                    className={cn(
                      TEXT_LABEL,
                      "me-auto flex items-center gap-2 px-1"
                    )}
                  >
                    <Spinner className="size-3" /> Thinking…
                  </span>
                )}
              </li>
            ))}
          </ul>
        )}
        <div ref={threadEnd} />
      </div>

      <div className="border-t p-3">
        <Textarea
          data-testid="chat-input"
          value={message}
          rows={3}
          className="resize-none bg-background"
          onChange={(event) => setMessage(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter" && !event.shiftKey) {
              event.preventDefault();
              void handleSend();
            }
          }}
          placeholder="Ask a question, or describe a dashboard to create"
        />
        <div className="mt-2 flex items-center gap-2">
          <Button
            data-testid="chat-send"
            type="button"
            size="sm"
            onClick={() => void handleSend()}
            disabled={sendChat.isPending}
          >
            Send
          </Button>
          {/* One pending affordance per request: the thread carries it, where
              the answer is about to appear. A second spinner beside the button
              said the same thing twice. */}
          <span className={TEXT_LABEL}>
            Enter sends, Shift+Enter breaks the line
          </span>
        </div>
      </div>
    </aside>
  );
}

function ChatAnswer({ reply }: { reply: ChatReply }) {
  const tool = toolUsed(reply);
  const created = reply.created;
  const built = [
    created?.metric ? `metric ${created.metric}` : null,
    created?.widgets.length
      ? `${created.widgets.length === 1 ? "widget" : "widgets"} ${created.widgets.join(", ")}`
      : null,
    created?.dashboard ? `dashboard ${created.dashboard}` : null,
  ].filter((entry): entry is string => entry !== null);

  return (
    <div className="me-auto flex w-full flex-col gap-2 rounded-2xl rounded-es-sm bg-background px-3 py-2 shadow-xs">
      {tool ? (
        <Badge variant="secondary" className="w-fit font-mono">
          {tool}
        </Badge>
      ) : null}

      <p className={cn(TEXT_BODY, "leading-relaxed")}>{reply.reply}</p>

      {reply.result ? (
        <div className="max-h-64 overflow-auto rounded-md border">
          <CustomTable result={reply.result} />
        </div>
      ) : null}

      {built.length ? (
        <p className={TEXT_LABEL}>Built {built.join(" · ")}</p>
      ) : null}

      {reply.skipped?.map((skip) => (
        <p
          key={`${skip.kind}:${skip.name}`}
          className={TEXT_LABEL}
        >
          {skip.name} already exists, left as it was
        </p>
      ))}
    </div>
  );
}
