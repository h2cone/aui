import { Bridge } from "../protocol.js";

export function registerOpenCode(bridge: Bridge) {
  bridge.register({
    name: "opencode",
    async handle() {
      return { message: "OpenCode adapter not configured yet." };
    },
  });
}
