import { Bridge } from "../protocol.js";
import { streamOpenAIChat } from "./openai_stream.js";

const DEFAULT_MODEL = "gpt-4o-mini";

export function registerCodex(bridge: Bridge) {
  bridge.register({
    name: "codex-cli",
    async handle(request, context) {
      const apiKey = process.env.OPENAI_API_KEY;
      if (!apiKey) {
        context.error("Missing OPENAI_API_KEY");
        return;
      }

      const model =
        process.env.AUI_CODEX_MODEL ??
        process.env.OPENAI_MODEL ??
        DEFAULT_MODEL;

      await streamOpenAIChat(request, context, {
        apiKey,
        baseURL: process.env.OPENAI_BASE_URL,
        model,
      });
    },
  });
}
