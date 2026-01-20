import { Bridge } from "../protocol.js";
import { streamOpenAIChat } from "./openai_stream.js";

const DEFAULT_MODEL = "gpt-4o-mini";

export function registerOpenCode(bridge: Bridge) {
  bridge.register({
    name: "opencode-cli",
    async handle(request, context) {
      const apiKey = process.env.OPENCODE_API_KEY;
      const baseURL = process.env.OPENCODE_BASE_URL;
      if (!apiKey || !baseURL) {
        context.error("Missing OPENCODE_API_KEY or OPENCODE_BASE_URL");
        return;
      }

      const model = process.env.AUI_OPENCODE_MODEL ?? DEFAULT_MODEL;

      await streamOpenAIChat(request, context, {
        apiKey,
        baseURL,
        model,
      });
    },
  });
}
