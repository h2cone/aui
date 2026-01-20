import { Bridge } from "../protocol.js";

export function registerClaude(bridge: Bridge) {
  bridge.register({
    name: "claude",
    async handle() {
      return { message: "Claude adapter not configured yet." };
    },
  });
}
