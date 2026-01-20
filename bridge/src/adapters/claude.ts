import Anthropic from "@anthropic-ai/sdk";
import { Bridge } from "../protocol.js";

const DEFAULT_MODEL = "claude-3-5-sonnet-20241022";
const DEFAULT_MAX_TOKENS = 1024;

function parseIntEnv(value: string | undefined, fallback: number) {
  if (!value) {
    return fallback;
  }
  const parsed = Number.parseInt(value, 10);
  return Number.isFinite(parsed) ? parsed : fallback;
}

export function registerClaude(bridge: Bridge) {
  bridge.register({
    name: "claude-code",
    async handle(request, context) {
      const apiKey = process.env.ANTHROPIC_API_KEY;
      if (!apiKey) {
        context.error("Missing ANTHROPIC_API_KEY");
        return;
      }

      const text = request.params?.text ?? "";
      const model = process.env.AUI_CLAUDE_MODEL ?? DEFAULT_MODEL;
      const maxTokens = parseIntEnv(
        process.env.AUI_CLAUDE_MAX_TOKENS,
        DEFAULT_MAX_TOKENS,
      );

      const client = new Anthropic({ apiKey });
      const stream = client.messages.stream({
        model,
        max_tokens: maxTokens,
        messages: [{ role: "user", content: text }],
      });

      for await (const event of stream) {
        switch (event.type) {
          case "content_block_delta":
            if ("text" in event.delta) {
              context.textDelta(event.delta.text);
            }
            break;
          case "content_block_start":
            if (
              "content_block" in event &&
              event.content_block?.type === "tool_use"
            ) {
              const name = event.content_block.name ?? "tool";
              const input = JSON.stringify(event.content_block.input ?? {});
              context.toolStart(name, input);
            }
            break;
          default:
            break;
        }
      }

      const finalMessage = await stream.finalMessage().catch(() => null);
      if (finalMessage?.usage) {
        context.tokenUsage(
          finalMessage.usage.input_tokens ?? 0,
          finalMessage.usage.output_tokens ?? 0,
        );
      }
    },
  });
}
