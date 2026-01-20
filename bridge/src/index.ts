import { createBridge } from "./protocol.js";
import { registerClaude } from "./adapters/claude.js";
import { registerCodex } from "./adapters/codex.js";
import { registerGemini } from "./adapters/gemini.js";
import { registerOpenCode } from "./adapters/opencode.js";

const bridge = createBridge();
registerClaude(bridge);
registerCodex(bridge);
registerGemini(bridge);
registerOpenCode(bridge);
bridge.start();
