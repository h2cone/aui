export type BridgeAdapter = {
  name: string;
  handle: (payload: unknown) => Promise<unknown>;
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
      process.stdin.resume();
    },
  };
}
