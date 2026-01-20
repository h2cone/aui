import { Bridge } from "../protocol.js";

export function registerGemini(bridge: Bridge) {
  bridge.register({
    name: "gemini",
    async handle() {
      return { message: "Gemini adapter not configured yet." };
    },
  });
}
