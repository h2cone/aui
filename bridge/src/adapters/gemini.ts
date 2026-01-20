import { GoogleGenerativeAI } from "@google/generative-ai";
import { Bridge } from "../protocol.js";

const DEFAULT_MODEL = "gemini-1.5-pro";

export function registerGemini(bridge: Bridge) {
  bridge.register({
    name: "gemini-cli",
    async handle(request, context) {
      const apiKey = process.env.GEMINI_API_KEY ?? process.env.GOOGLE_API_KEY;
      if (!apiKey) {
        context.error("Missing GEMINI_API_KEY or GOOGLE_API_KEY");
        return;
      }

      const text = request.params?.text ?? "";
      const modelName = process.env.AUI_GEMINI_MODEL ?? DEFAULT_MODEL;
      const client = new GoogleGenerativeAI(apiKey);
      const model = client.getGenerativeModel({ model: modelName });

      const result = await model.generateContentStream(text);
      for await (const chunk of result.stream) {
        const delta = chunk.text();
        if (delta) {
          context.textDelta(delta);
        }
      }

      const response = await result.response;
      const usage = response.usageMetadata;
      if (usage) {
        context.tokenUsage(
          usage.promptTokenCount ?? 0,
          usage.candidatesTokenCount ?? 0,
        );
      }
    },
  });
}
