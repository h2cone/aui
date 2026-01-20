import { Bridge } from "../protocol.js";

export function registerCodex(bridge: Bridge) {
  bridge.register({
    name: "codex",
    async handle() {
      return { message: "Codex adapter not configured yet." };
    },
  });
}
