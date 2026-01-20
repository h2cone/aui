import OpenAI from "openai";
import type { BridgeContext, BridgeRequest } from "../protocol.js";

type StreamConfig = {
  apiKey: string;
  baseURL?: string;
  model: string;
};

type ToolCall = {
  name: string;
  args: string;
};

export async function streamOpenAIChat(
  request: BridgeRequest,
  context: BridgeContext,
  config: StreamConfig,
) {
  const client = new OpenAI({
    apiKey: config.apiKey,
    baseURL: config.baseURL,
  });
  const text = request.params?.text ?? "";

  const stream = await client.chat.completions.create({
    model: config.model,
    stream: true,
    stream_options: { include_usage: true },
    messages: [{ role: "user", content: text }],
  });

  const toolCalls = new Map<string, ToolCall>();
  const trackToolCall = (id: string, name?: string, args?: string) => {
    const existing = toolCalls.get(id) ?? { name: name ?? "tool", args: "" };
    if (name) {
      existing.name = name;
    }
    if (args) {
      existing.args += args;
    }
    toolCalls.set(id, existing);
  };

  for await (const chunk of stream) {
    const choice = chunk.choices?.[0];
    const delta = choice?.delta;
    const content = delta?.content;
    if (content) {
      context.textDelta(content);
    }

    const toolCallDeltas = delta?.tool_calls ?? [];
    for (const call of toolCallDeltas) {
      const callId = call.id ?? "tool-call";
      trackToolCall(callId, call.function?.name, call.function?.arguments);
    }

    if (delta?.function_call) {
      trackToolCall(
        "function-call",
        delta.function_call.name,
        delta.function_call.arguments,
      );
    }

    if (choice?.finish_reason === "tool_calls" && toolCalls.size > 0) {
      for (const tool of toolCalls.values()) {
        context.toolStart(tool.name, tool.args);
      }
      toolCalls.clear();
    }

    if (chunk.usage) {
      context.tokenUsage(
        chunk.usage.prompt_tokens ?? 0,
        chunk.usage.completion_tokens ?? 0,
      );
    }
  }

  if (toolCalls.size > 0) {
    for (const tool of toolCalls.values()) {
      context.toolStart(tool.name, tool.args);
    }
  }
}
