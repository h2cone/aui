import readline from "node:readline";

export type BridgeRequest = {
  id: string;
  method: string;
  params?: {
    agent_id?: string;
    text?: string;
    attachments?: Array<{ name: string; path?: string }>;
    context?: { cwd?: string };
  };
};

export type BridgeEvent =
  | { type: "text_delta"; delta: string }
  | { type: "tool_start"; name: string; input: string }
  | { type: "tool_result"; name: string; output: string }
  | { type: "token_usage"; input: number; output: number }
  | { type: "done" }
  | { type: "error"; message: string };

export type BridgeContext = {
  emit: (event: BridgeEvent) => void;
  textDelta: (delta: string) => void;
  toolStart: (name: string, input: string) => void;
  toolResult: (name: string, output: string) => void;
  tokenUsage: (input: number, output: number) => void;
  done: () => void;
  error: (message: string) => void;
};

type BridgeAdapter = {
  name: string;
  handle: (payload: BridgeRequest, context: BridgeContext) => Promise<void>;
};

type BridgeResponse = {
  id: string;
  event: BridgeEvent;
};

export type Bridge = {
  register: (adapter: BridgeAdapter) => void;
  start: () => void;
};

export function createBridge(): Bridge {
  const adapters = new Map<string, BridgeAdapter>();

  return {
    register(adapter: BridgeAdapter) {
      adapters.set(adapter.name, adapter);
    },
    start() {
      const rl = readline.createInterface({
        input: process.stdin,
        crlfDelay: Infinity,
      });

      rl.on("line", async (line) => {
        const trimmed = line.trim();
        if (!trimmed) {
          return;
        }

        let request: BridgeRequest;
        try {
          request = JSON.parse(trimmed) as BridgeRequest;
        } catch (error) {
          writeResponse({
            id: "unknown",
            event: { type: "error", message: `Invalid JSON: ${String(error)}` },
          });
          return;
        }

        if (request.method !== "send") {
          writeResponse({
            id: request.id ?? "unknown",
            event: {
              type: "error",
              message: `Unsupported method: ${request.method}`,
            },
          });
          return;
        }

        const agentId = request.params?.agent_id;
        if (!agentId) {
          writeResponse({
            id: request.id ?? "unknown",
            event: { type: "error", message: "Missing agent_id" },
          });
          return;
        }

        const adapter = adapters.get(agentId);
        if (!adapter) {
          writeResponse({
            id: request.id ?? "unknown",
            event: { type: "error", message: `Unknown adapter: ${agentId}` },
          });
          return;
        }

        let doneSent = false;
        const emit = (event: BridgeEvent) => {
          if (doneSent) {
            return;
          }
          if (event.type === "done" || event.type === "error") {
            doneSent = true;
          }
          writeResponse({ id: request.id, event });
        };

        const context: BridgeContext = {
          emit,
          textDelta: (delta) => emit({ type: "text_delta", delta }),
          toolStart: (name, input) => emit({ type: "tool_start", name, input }),
          toolResult: (name, output) =>
            emit({ type: "tool_result", name, output }),
          tokenUsage: (input, output) =>
            emit({ type: "token_usage", input, output }),
          done: () => emit({ type: "done" }),
          error: (message) => emit({ type: "error", message }),
        };

        try {
          await adapter.handle(request, context);
          if (!doneSent) {
            context.done();
          }
        } catch (error) {
          if (!doneSent) {
            context.error(`Adapter error: ${String(error)}`);
          }
        }
      });
    },
  };
}

function writeResponse(response: BridgeResponse) {
  process.stdout.write(`${JSON.stringify(response)}\n`);
}
