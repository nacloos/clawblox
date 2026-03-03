import { Agent } from "@mariozechner/pi-agent-core";
import { getModel } from "@mariozechner/pi-ai";
import { createCodingTools, calculateContextTokens, shouldCompact } from "@mariozechner/pi-coding-agent";
import { ProcessTerminal, TUI, Input, truncateToWidth, visibleWidth } from "@mariozechner/pi-tui";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

function loadDotEnvIntoProcessEnv(envPath) {
	if (!existsSync(envPath)) return;
	const content = readFileSync(envPath, "utf8");
	for (const rawLine of content.split(/\r?\n/)) {
		const line = rawLine.trim();
		if (!line || line.startsWith("#")) continue;
		const eq = line.indexOf("=");
		if (eq <= 0) continue;
		const key = line.slice(0, eq).trim();
		if (!key || process.env[key] !== undefined) continue;
		let value = line.slice(eq + 1).trim();
		if ((value.startsWith('"') && value.endsWith('"')) || (value.startsWith("'") && value.endsWith("'"))) {
			value = value.slice(1, -1);
		}
		process.env[key] = value;
	}
}

function deepDiff(prev, curr) {
	if (prev === curr) return undefined;
	if (prev == null || curr == null || typeof prev !== typeof curr) return curr;
	if (typeof curr !== "object") return prev === curr ? undefined : curr;
	if (Array.isArray(curr)) return JSON.stringify(prev) === JSON.stringify(curr) ? undefined : curr;

	const result = {};
	for (const key of new Set([...Object.keys(prev), ...Object.keys(curr)])) {
		if (!(key in curr)) {
			result[key] = null;
			continue;
		}
		if (!(key in prev)) {
			result[key] = curr[key];
			continue;
		}
		const d = deepDiff(prev[key], curr[key]);
		if (d !== undefined) result[key] = d;
	}
	return Object.keys(result).length > 0 ? result : undefined;
}

function diffEntityArrays(prev, curr) {
	const prevMap = new Map();
	for (const e of prev) prevMap.set(e.id, JSON.stringify(e));
	const currMap = new Map();
	for (const e of curr) currMap.set(e.id, JSON.stringify(e));

	const changed = [];
	const added = [];
	const removed = [];

	for (const e of curr) {
		const prevStr = prevMap.get(e.id);
		if (prevStr == null) {
			added.push(e);
			continue;
		}
		if (prevStr !== JSON.stringify(e)) changed.push(e);
	}
	for (const id of prevMap.keys()) {
		if (!currMap.has(id)) removed.push(id);
	}

	if (changed.length === 0 && added.length === 0 && removed.length === 0) return undefined;
	const result = {};
	if (changed.length > 0) result.changed = changed;
	if (added.length > 0) result.added = added;
	if (removed.length > 0) result.removed = removed;
	return result;
}

function diffObservation(prev, curr) {
	if (!prev) return curr;
	const diff = { tick: curr.tick, game_status: curr.game_status, events: curr.events || [] };
	const playerDiff = deepDiff(prev.player, curr.player);
	if (playerDiff) diff.player = playerDiff;
	const otherDiff = deepDiff(prev.other_players, curr.other_players);
	if (otherDiff) diff.other_players = otherDiff;
	if (prev.world?.entities && curr.world?.entities) {
		const entDiff = diffEntityArrays(prev.world.entities, curr.world.entities);
		if (entDiff) diff.world = { entities: entDiff };
	} else {
		const worldDiff = deepDiff(prev.world, curr.world);
		if (worldDiff) diff.world = worldDiff;
	}
	return diff;
}

function wrapLine(line, width) {
	if (width <= 0) return [""];
	if (!line) return [""];
	const out = [];
	const logicalLines = String(line).split(/\r?\n/);
	for (const logical of logicalLines) {
		if (!logical) {
			out.push("");
			continue;
		}
		let s = logical;
		while (s.length > width) {
			out.push(s.slice(0, width));
			s = s.slice(width);
		}
		out.push(s);
	}
	return out.length > 0 ? out : [""];
}

function padOrTrimToWidth(text, width) {
	const clipped = truncateToWidth(String(text || ""), width, "", false);
	const w = visibleWidth(clipped);
	return w < width ? clipped + " ".repeat(width - w) : clipped;
}

class OnePaneLogs {
	constructor(terminal, state) {
		this.terminal = terminal;
		this.state = state;
	}

	invalidate() {}

	render(width) {
		const panelWidth = Math.max(16, width);
		const rows = Math.max(8, this.terminal.rows - 4);
		const all = [];
		for (const line of this.state.lines) all.push(...wrapLine(line, panelWidth));
		if (this.state.liveLine) all.push(...wrapLine(this.state.liveLine, panelWidth));
		if (this.state.pinnedUser) all.push(...wrapLine(this.state.pinnedUser, panelWidth));

		const header = padOrTrimToWidth(` Agent (${this.state.busy ? "busy" : "idle"}) `, panelWidth);
		const body = all.slice(-(rows - 1));
		while (body.length < rows - 1) body.unshift("");
		return [header, ...body.map((line) => padOrTrimToWidth(line, panelWidth))];
	}
}

function loadCodexAccessToken(scriptDir) {
	const candidates = [path.join(process.cwd(), "auth.json"), path.join(scriptDir, "auth.json")];
	for (const p of candidates) {
		if (!existsSync(p)) continue;
		try {
			const auth = JSON.parse(readFileSync(p, "utf8"));
			const creds = auth?.["openai-codex"];
			if (creds && typeof creds.access === "string" && creds.access.length > 0) return creds.access;
		} catch {}
	}
	return undefined;
}

function now() {
	return new Date().toISOString().slice(11, 19);
}

function sleep(ms) {
	return new Promise((resolve) => setTimeout(resolve, ms));
}

async function fetchJson(url, init = {}) {
	const res = await fetch(url, init);
	if (!res.ok) throw new Error(`${res.status} ${res.statusText}`);
	return await res.json();
}

const scriptPath = fileURLToPath(import.meta.url);
const scriptDir = path.dirname(scriptPath);
const scriptName = path.basename(scriptPath, path.extname(scriptPath));
loadDotEnvIntoProcessEnv(path.resolve(scriptDir, "..", ".env"));

const resultsDir = path.join(scriptDir, "results", scriptName);
mkdirSync(resultsDir, { recursive: true });

const conversationPath = path.join(resultsDir, "conversation.json");
const systemPromptPath = path.join(resultsDir, "system-prompt.md");

const agentDir = process.env.WORKSPACE_DIR
	? path.resolve(process.env.WORKSPACE_DIR)
	: path.join(scriptDir, "workspace", "agent");
const contextFiles = ["SOUL.md", "IDENTITY.md", "SEMANTIC_MEMORY.md", "EPISODIC_MEMORY.md"];
mkdirSync(agentDir, { recursive: true });
for (const file of contextFiles) {
	const dest = path.join(agentDir, file);
	if (!existsSync(dest)) {
		const templatePath = path.join(scriptDir, "template", "agent", file);
		const template = existsSync(templatePath) ? readFileSync(templatePath, "utf8") : "";
		writeFileSync(dest, template);
	}
}

function loadContextFile(name) {
	return readFileSync(path.join(agentDir, name), "utf8").trim();
}

const worldBaseUrl = process.env.WORLD_BASE_URL || "http://localhost:8080";
const safeDefaultAgentName = `bot${process.pid}`.slice(0, 16);
const worldAgentName = (process.env.WORLD_AGENT_NAME || safeDefaultAgentName).slice(0, 16);

let worldSession = "";
let worldAgentId = "";
let actionAutoLoop = true;
let isClosing = false;
let contextFlushPending = false;

async function joinWorldOrThrow() {
	let lastErr = null;
	for (let i = 0; i < 5; i++) {
		try {
			const join = await fetchJson(`${worldBaseUrl}/join?name=${encodeURIComponent(worldAgentName)}`, { method: "POST" });
			if (!join?.session) throw new Error("join response missing session");
			worldSession = String(join.session);
			worldAgentId = String(join.agent_id || "");
			return;
		} catch (err) {
			lastErr = err;
			await sleep(300);
		}
	}
	throw new Error(`Failed to join world at ${worldBaseUrl}: ${lastErr instanceof Error ? lastErr.message : String(lastErr)}`);
}

function buildWorkspaceContext() {
	const lines = [];
	lines.push(
		"## Workspace",
		"",
		`Your workspace is ${agentDir}. Only work inside this directory.`,
		"Be polite and avoid working on your memory files when interacting with someone else.",
		"",
		"# Project Context",
		"",
		"The following files are loaded from your workspace. They are yours to evolve.",
		"If SOUL.md is present, embody its persona and tone.",
		""
	);
	for (const file of contextFiles) {
		const filePath = path.join(agentDir, file);
		const content = loadContextFile(file);
		if (content) lines.push("---", "", `**${file}** (${filePath})`, "", content, "");
	}
	return lines.join("\n");
}

function buildSystemPrompt() {
	const base = process.env.PI_SYSTEM_PROMPT || "";
	const lines = [
		"/no_think",
		base,
		"You are a being living in your world.",
		"",
		"## Live session",
		"",
		`World base URL: ${worldBaseUrl}`,
		`Session key (X-Session): ${worldSession}`,
		`Agent id: ${worldAgentId || "(unknown)"}`,
		"",
		`At the start of a session, fetch world commands with: curl -sS -H \"X-Session: ${worldSession}\" ${worldBaseUrl}/skill.md`,
		"You will receive world observations. Choose and execute your next action using /input.",
		`Act: curl -sS -X POST -H \"X-Session: ${worldSession}\" -H \"Content-Type: application/json\" -d '<json>' ${worldBaseUrl}/input`,
		"Always make a bash tool call to /input. Never respond with text only.",
		"",
		"## Memory",
		"",
		"You have workspace files. Write to them when something feels worth remembering. It's your call.",
		"",
		buildWorkspaceContext(),
	];
	return lines.join("\n");
}

await joinWorldOrThrow();

const systemPrompt = buildSystemPrompt();
writeFileSync(systemPromptPath, systemPrompt);

const codexAccessToken = loadCodexAccessToken(scriptDir);
const anthropicApiKey = process.env.ANTHROPIC_API_KEY;
const modelProvider = process.env.PI_PROVIDER || "anthropic";

let defaultModelName;
if (modelProvider === "openai-codex") defaultModelName = "gpt-5.3-codex";
else if (modelProvider === "ollama") defaultModelName = "lfm2.5-thinking:1.2b-bf16";
else defaultModelName = "claude-opus-4-6";
const modelName = process.env.PI_MODEL || defaultModelName;

let model;
if (modelProvider === "ollama") {
	const ollamaBaseUrl = process.env.OLLAMA_BASE_URL || "http://localhost:11434/v1";
	model = {
		id: modelName,
		name: modelName,
		api: "openai-completions",
		provider: "ollama",
		baseUrl: ollamaBaseUrl,
		reasoning: modelName.includes("thinking"),
		input: ["text"],
		cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 },
		contextWindow: 32768,
		maxTokens: 8192,
		headers: {},
		compat: {
			supportsStore: false,
			supportsDeveloperRole: false,
			supportsReasoningEffort: false,
			supportsUsageInStreaming: false,
			maxTokensField: "max_tokens",
			requiresToolResultName: false,
			requiresAssistantAfterToolResult: false,
			requiresThinkingAsText: false,
			requiresMistralToolIds: false,
			thinkingFormat: "openai",
			supportsStrictMode: false,
		},
	};
} else {
	model = getModel(modelProvider, modelName);
}

if (modelProvider === "openai-codex" && !codexAccessToken) {
	throw new Error("Missing Codex OAuth token. Run: npx @mariozechner/pi-ai login openai-codex");
}
if (modelProvider === "anthropic" && !anthropicApiKey) {
	throw new Error("Missing ANTHROPIC_API_KEY (set env var or add it to .env)");
}

const agent = new Agent({
	initialState: {
		systemPrompt,
		model,
		thinkingLevel: "off",
		tools: createCodingTools(process.cwd()),
	},
	getApiKey: (provider) => {
		if (provider === "openai-codex") return codexAccessToken;
		if (provider === "anthropic") return anthropicApiKey;
		if (provider === "ollama") return "ollama";
		return undefined;
	},
});
agent.setSteeringMode("all");

const state = {
	busy: false,
	lines: [],
	liveLine: "",
	pinnedUser: "",
};

const FLUSH_PROMPT = "[SYSTEM] Context nearly full. Write anything worth keeping to your workspace memory files, then confirm ready.";

const terminal = new ProcessTerminal();
const tui = new TUI(terminal, true);
const logs = new OnePaneLogs(terminal, state);
const inputBox = new Input();

function trimBuffer(arr, max = 250) {
	if (arr.length > max) arr.splice(0, arr.length - max);
}

function addLine(text) {
	state.lines.push(`[${now()}] ${text}`);
	trimBuffer(state.lines);
	tui.requestRender();
}

function persistConversation() {
	writeFileSync(conversationPath, JSON.stringify(agent.state.messages, null, 2));
}

async function runPrompt(text) {
	if (isClosing || state.busy) return;
	state.busy = true;
	state.liveLine = "";
	tui.requestRender();
	try {
		await agent.prompt(text);
	} catch (error) {
		addLine(`[error] ${error instanceof Error ? error.message : String(error)}`);
	} finally {
		state.busy = false;
		state.liveLine = "";
		tui.requestRender();
		if (actionAutoLoop && !isClosing) setTimeout(() => void driveLoop(), 300);
	}
}

async function runContinue() {
	if (isClosing || state.busy) return;
	state.busy = true;
	tui.requestRender();
	try {
		await agent.continue();
	} catch (error) {
		addLine(`[error] ${error instanceof Error ? error.message : String(error)}`);
	} finally {
		state.busy = false;
		state.liveLine = "";
		tui.requestRender();
		if (actionAutoLoop && !isClosing) setTimeout(() => void driveLoop(), 300);
	}
}

async function driveLoop() {
	if (isClosing || state.busy) return;
	await runContinue();
}

agent.subscribe((event) => {
	persistConversation();

	if (event.type === "message_start" && event.message.role === "assistant") {
		state.liveLine = "assistant>";
		tui.requestRender();
		return;
	}
	if (event.type === "message_update" && event.assistantMessageEvent.type === "text_delta") {
		state.liveLine += event.assistantMessageEvent.delta;
		tui.requestRender();
		return;
	}
	if (event.type === "message_update" && event.assistantMessageEvent.type === "toolcall_end") {
		const tc = event.assistantMessageEvent.toolCall;
		addLine(`[tool] ${tc.name}(${JSON.stringify(tc.arguments)})`);
	}
	if (event.type === "message_end" && event.message.role === "assistant") {
		const txt = event.message.content.filter((c) => c.type === "text").map((c) => c.text).join("").trim();
		if (txt) addLine(`assistant> ${txt}`);
		const err = event.message.errorMessage;
		if (err) addLine(`[error] ${err}`);

		if (event.message.usage && !contextFlushPending) {
			const tokens = calculateContextTokens(event.message.usage);
			const contextWindow = agent.state.model.contextWindow;
			if (shouldCompact(tokens, contextWindow, { enabled: true, reserveTokens: 16384, keepRecentTokens: 0 })) {
				contextFlushPending = true;
				addLine("[context] Approaching limit. Flushing memories...");
				agent.followUp({ role: "user", content: [{ type: "text", text: FLUSH_PROMPT }], timestamp: Date.now() });
			}
		}

		state.liveLine = "";
		state.pinnedUser = "";
		tui.requestRender();
	}
	if (event.type === "agent_end" && contextFlushPending) {
		contextFlushPending = false;
		agent.reset();
		agent.setSystemPrompt(buildSystemPrompt());
		addLine("[context] Session reset.");
	}
});

async function handleCommand(line) {
	if (line === "/quit" || line === "/exit") {
		isClosing = true;
		persistConversation();
		tui.stop();
		process.exit(0);
		return;
	}
	if (line === "/reset") {
		if (state.busy) agent.abort();
		agent.reset();
		agent.setSystemPrompt(buildSystemPrompt());
		contextFlushPending = false;
		addLine("Context cleared.");
		return;
	}
	if (line === "/session") {
		addLine(`session=${worldSession}`);
		return;
	}
	if (line === "/loop on") {
		actionAutoLoop = true;
		addLine("Auto-loop enabled.");
		if (!state.busy) void driveLoop();
		return;
	}
	if (line === "/loop off") {
		actionAutoLoop = false;
		addLine("Auto-loop disabled.");
		return;
	}
	if (line.startsWith("/ask ")) {
		const text = line.slice(5).trim();
		if (!text) return;
		state.pinnedUser = `you> ${text}`;
		addLine(`you> ${text}`);
		if (state.busy) {
			agent.steer({ role: "user", content: [{ type: "text", text }], timestamp: Date.now() });
			addLine("Queued message.");
			return;
		}
		void runPrompt(text);
		return;
	}
	if (line.startsWith("/a ")) {
		const text = line.slice(3).trim();
		if (!text) return;
		state.pinnedUser = `you> ${text}`;
		addLine(`you> ${text}`);
		if (state.busy) {
			agent.steer({ role: "user", content: [{ type: "text", text }], timestamp: Date.now() });
			addLine("Queued message.");
			return;
		}
		void runPrompt(text);
		return;
	}
	if (line === "/a" || line === "/ask") {
		addLine("Usage: /ask <message>");
		return;
	}

	state.pinnedUser = `you> ${line}`;
	addLine(`you> ${line}`);
	if (state.busy) {
		agent.steer({ role: "user", content: [{ type: "text", text: line }], timestamp: Date.now() });
		addLine("Queued message.");
		return;
	}
	void runPrompt(line);
}

tui.addChild(logs);
tui.addChild(inputBox);
tui.setFocus(inputBox);

inputBox.onSubmit = (value) => {
	const line = value.trim();
	inputBox.setValue("");
	if (!line) {
		tui.requestRender();
		return;
	}
	void handleCommand(line);
};

process.on("SIGINT", () => {
	if (state.busy) {
		agent.abort();
		state.busy = false;
		addLine("Aborted.");
		return;
	}
	void handleCommand("/quit");
});

addLine(`Single-agent ready with model "${modelName}" (${modelProvider}).`);
addLine(`World joined: ${worldBaseUrl} agent=${worldAgentName}`);
addLine(`Session key: ${worldSession}`);
addLine("Commands: /reset, /quit, /session, /loop on|off, /ask <msg> (or plain text)");

tui.start();
void runPrompt("Fetch the skill commands and start acting in the world.");
