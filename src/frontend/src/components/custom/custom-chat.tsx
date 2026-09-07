import { useState } from "react";

import type { ChatCreated, ChatReply } from "@/api/custom-client";
import { CustomTable } from "@/components/custom/custom-table";
import { Button } from "@/components/ui/button";
import { Spinner } from "@/components/ui/spinner";
import { Textarea } from "@/components/ui/textarea";
import { useSendChat } from "@/queries/custom";

export interface CustomChatProps {
  onCreated: (created: ChatCreated) => void;
}

interface Exchange {
  id: number;
  question: string;
  reply?: ChatReply;
  error?: string;
}

export function CustomChat({ onCreated }: CustomChatProps) {
  const [message, setMessage] = useState("");
  const [exchanges, setExchanges] = useState<Exchange[]>([]);
  const sendChat = useSendChat();

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
    <div>
      <ul>
        {exchanges.map((exchange) => (
          <li key={exchange.id}>
            <p>{exchange.question}</p>
            {exchange.reply && (
              <>
                <p>{exchange.reply.reply}</p>
                {exchange.reply.result && (
                  <CustomTable result={exchange.reply.result} />
                )}
                {exchange.reply.skipped?.map((skip) => (
                  <p key={`${skip.kind}:${skip.name}`}>
                    {skip.name} already exists, left as it was
                  </p>
                ))}
              </>
            )}
            {exchange.error && <p role="alert">{exchange.error}</p>}
          </li>
        ))}
      </ul>
      <Textarea
        value={message}
        onChange={(event) => setMessage(event.target.value)}
        placeholder="Ask a question, or describe a dashboard to create"
      />
      <div className="flex items-center gap-2">
        <Button
          type="button"
          onClick={() => void handleSend()}
          disabled={sendChat.isPending}
        >
          Send
        </Button>
        {sendChat.isPending && (
          <span className="flex items-center gap-2 text-xs text-muted-foreground">
            <Spinner className="size-3" /> Sending…
          </span>
        )}
      </div>
    </div>
  );
}
