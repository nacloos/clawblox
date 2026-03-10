import type { ExtensionAPI } from "@mariozechner/pi-coding-agent";
import { Type } from "@sinclair/typebox";
import { mkdirSync, writeFileSync } from "node:fs";
import { dirname } from "node:path";

const AUTO_LOOP_STEER = "continue";
const PROJECT_CONTEXT_SECTION = /(?:^|\n)# Project Context\b[\s\S]*$/m;

function writeSystemPromptSnapshot(prompt: string) {
  const outputPath = process.env.PI_SYSTEM_PROMPT_SNAPSHOT_FILE;
  if (!outputPath || !prompt.trim()) return;
  mkdirSync(dirname(outputPath), { recursive: true });
  writeFileSync(outputPath, prompt.endsWith("\n") ? prompt : `${prompt}\n`, "utf8");
}

function stripProjectContext(prompt: string): string {
  return prompt.replace(PROJECT_CONTEXT_SECTION, "").trimEnd();
}

export default function (pi: ExtensionAPI) {
  let autoLoopEnabled = true;

  pi.on("before_agent_start", async (event) => {
    const sanitizedPrompt = stripProjectContext(event.systemPrompt);
    writeSystemPromptSnapshot(sanitizedPrompt);
    return { systemPrompt: sanitizedPrompt };
  });

  pi.registerCommand("agent-auto", {
    description: "Control the generic continue auto-loop: /agent-auto on|off|status",
    handler: async (args, ctx) => {
      const mode = (args || "").trim().toLowerCase();
      if (mode === "off") {
        autoLoopEnabled = false;
        if (ctx.hasUI) ctx.ui.setStatus("agent-loop", "⏸ agent");
        ctx.ui.notify("Agent auto-loop disabled", "info");
        return;
      }
      if (mode === "on" || mode === "") {
        autoLoopEnabled = true;
        if (ctx.hasUI) ctx.ui.setStatus("agent-loop", "♾ agent");
        ctx.ui.notify("Agent auto-loop enabled", "info");
        if (ctx.isIdle() && !ctx.hasPendingMessages()) {
          pi.sendUserMessage(AUTO_LOOP_STEER, { deliverAs: "steer" });
        }
        return;
      }
      if (mode === "status") {
        ctx.ui.notify(`Agent auto-loop is ${autoLoopEnabled ? "enabled" : "disabled"}`, "info");
        return;
      }
      ctx.ui.notify("Usage: /agent-auto on|off|status", "warning");
    },
  });

  pi.on("agent_end", async (_event, ctx) => {
    if (ctx.hasUI) ctx.ui.setStatus("agent-loop", autoLoopEnabled ? "♾ agent" : "⏸ agent");
    if (!autoLoopEnabled) return;
    if (!ctx.isIdle()) return;
    if (ctx.hasPendingMessages()) return;
    pi.sendUserMessage(AUTO_LOOP_STEER, { deliverAs: "steer" });
  });

  pi.registerCommand("reload-runtime", {
    description: "Reload extensions, skills, prompts, and themes",
    handler: async (_args, ctx) => {
      await ctx.reload();
      return;
    },
  });

  pi.registerTool({
    name: "reload_runtime",
    label: "Reload Runtime",
    description: "Reload extensions, skills, prompts, and themes",
    parameters: Type.Object({}),
    async execute() {
      pi.sendUserMessage("/reload-runtime", { deliverAs: "followUp" });
      return {
        content: [{ type: "text", text: "Queued /reload-runtime as a follow-up command." }],
        details: {},
      };
    },
  });
}
