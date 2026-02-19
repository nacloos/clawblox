import { Agent } from "@mariozechner/pi-agent-core";
import { getModel } from "@mariozechner/pi-ai";
import { createCodingTools, calculateContextTokens, shouldCompact } from "@mariozechner/pi-coding-agent";
import { ProcessTerminal, TUI, Input, truncateToWidth, visibleWidth } from "@mariozechner/pi-tui";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { spawn } from "node:child_process";
import path from "node:path";

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
		if (
			(value.startsWith('"') && value.endsWith('"')) ||
			(value.startsWith("'") && value.endsWith("'"))
		) {
			value = value.slice(1, -1);
		}
		process.env[key] = value;
	}
}

function deepDiff(prev, curr) {
	if (prev === curr) return undefined;
	if (prev == null || curr == null || typeof prev !== typeof curr) return curr;
	if (typeof curr !== "object") return prev === curr ? undefined : curr;
	if (Array.isArray(curr)) {
		return JSON.stringify(prev) === JSON.stringify(curr) ? undefined : curr;
	}
	const result = {};
	for (const key of new Set([...Object.keys(prev), ...Object.keys(curr)])) {
		if (!(key in curr)) { result[key] = null; continue; }
		if (!(key in prev)) { result[key] = curr[key]; continue; }
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
		if (prevStr == null) { added.push(e); continue; }
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

function extractSpeakSegments(text) {
	const out = [];
	const re = /<(?:speak|s)>([\s\S]*?)<\/(?:speak|s)>/g;
	let m;
	while ((m = re.exec(text)) !== null) {
		const segment = m[1].trim();
		if (segment) out.push(segment);
	}
	return out;
}

function extractActivities(text) {
	const out = [];
	const re = /<step>([\s\S]*?)<\/step>/g;
	let m;
	while ((m = re.exec(text)) !== null) {
		const block = m[1];
		const actionMatch = /<action>([\s\S]*?)<\/action>/.exec(block);
		const obsMatch = /<observation>([\s\S]*?)<\/observation>/.exec(block);
		if (actionMatch || obsMatch) {
			out.push({
				action: actionMatch ? actionMatch[1].trim() : "",
				observation: obsMatch ? obsMatch[1].trim() : "",
			});
		}
	}
	return out;
}

function extractIntents(text) {
	const out = [];
	const re = /<intent>([\s\S]*?)<\/intent>/g;
	let m;
	while ((m = re.exec(text)) !== null) {
		const intent = m[1].trim();
		if (intent) out.push(intent);
	}
	return out;
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

function singleLine(line) {
	return String(line).replace(/\r?\n/g, " ");
}

function padOrTrimToWidth(text, width) {
	const clipped = truncateToWidth(String(text || ""), width, "", false);
	const w = visibleWidth(clipped);
	return w < width ? clipped + " ".repeat(width - w) : clipped;
}

class TwoPaneLogs {
	constructor(terminal, state) {
		this.terminal = terminal;
		this.state = state;
	}

	invalidate() {}

	render(width) {
		const sep = " | ";
		const panelWidth = Math.max(16, Math.floor((width - sep.length) / 2));
		const rows = Math.max(8, this.terminal.rows - 4);

		const leftAll = [];
		for (const line of this.state.speechLines) leftAll.push(...wrapLine(line, panelWidth));
		if (this.state.speechLiveLine) leftAll.push(...wrapLine(this.state.speechLiveLine, panelWidth));
		const pinnedLeftLine = this.state.speechPinnedUser ? singleLine(this.state.speechPinnedUser) : "";

		const rightAll = [];
		for (const line of this.state.actionLines) rightAll.push(...wrapLine(line, panelWidth));
		if (this.state.actionLiveLine) rightAll.push(...wrapLine(this.state.actionLiveLine, panelWidth));
		const pinnedRightLine = this.state.actionPinnedUser ? singleLine(this.state.actionPinnedUser) : "";

		const bodyRows = Math.max(1, rows - 1);
		const left = [...leftAll.slice(-bodyRows), pinnedLeftLine];
		const right = [...rightAll.slice(-bodyRows), pinnedRightLine];
		const count = Math.max(left.length, right.length, rows);

		const headerLeft = ` Speech (${this.state.speechBusy ? "busy" : "idle"}) `;
		const headerRight = ` Action (${this.state.actionBusy ? "busy" : "idle"}) `;
		const header = padOrTrimToWidth(headerLeft, panelWidth) + sep + padOrTrimToWidth(headerRight, panelWidth);

		const lines = [header];
		for (let i = 0; i < count; i++) {
			const l = padOrTrimToWidth(left[i] || "", panelWidth);
			const r = padOrTrimToWidth(right[i] || "", panelWidth);
			lines.push(l + sep + r);
		}
		return lines;
	}
}

class ElevenLabsRealtimePlayer {
	constructor({ apiKey, voiceId, modelId = "eleven_flash_v2_5", outputFormat = "pcm_16000" }) {
		this.apiKey = apiKey;
		this.voiceId = voiceId;
		this.modelId = modelId;
		this.outputFormat = outputFormat;
		this.ws = null;
		this.ffplay = null;
		this.keepaliveTimer = null;
		this.queue = [];
		this.processing = false;
		this.msgQueue = [];
		this.msgWaiter = null;
		this.closed = false;
	}

	startFfplay() {
		this.ffplay = spawn(
			"ffplay",
			[
				"-hide_banner",
				"-loglevel",
				"error",
				"-autoexit",
				"-nodisp",
				"-probesize",
				"32",
				"-analyzeduration",
				"0",
				"-fflags",
				"nobuffer",
				"-flags",
				"low_delay",
				"-f",
				"s16le",
				"-ar",
				"16000",
				"-ac",
				"1",
				"-i",
				"pipe:0",
			],
			{ stdio: ["pipe", "ignore", "ignore"] },
		);
	}

	cleanupConnection() {
		if (this.keepaliveTimer) clearInterval(this.keepaliveTimer);
		this.keepaliveTimer = null;
		if (this.ws && this.ws.readyState === WebSocket.OPEN) {
			try {
				this.ws.close();
			} catch {}
		}
		this.ws = null;
		if (this.ffplay) {
			try {
				this.ffplay.kill("SIGKILL");
			} catch {}
		}
		this.ffplay = null;
		this.msgQueue = [];
		this.msgWaiter = null;
	}

	async connect() {
		this.cleanupConnection();
		this.startFfplay();
		const wsUrl =
			`wss://api.elevenlabs.io/v1/text-to-speech/${this.voiceId}/stream-input` +
			`?model_id=${encodeURIComponent(this.modelId)}&output_format=${encodeURIComponent(this.outputFormat)}`;
		this.ws = new WebSocket(wsUrl);
		const ws = this.ws;

		await new Promise((resolve, reject) => {
			const onOpen = () => {
				cleanup();
				resolve();
			};
			const onError = (err) => {
				cleanup();
				reject(err instanceof Error ? err : new Error("WebSocket connection error"));
			};
			const cleanup = () => {
				ws.removeEventListener("open", onOpen);
				ws.removeEventListener("error", onError);
			};
			ws.addEventListener("open", onOpen);
			ws.addEventListener("error", onError);
		});

		ws.addEventListener("message", (event) => {
			let msg;
			try {
				msg = JSON.parse(String(event.data));
			} catch {
				return;
			}
			this.msgQueue.push(msg);
			if (this.msgWaiter) {
				const waiter = this.msgWaiter;
				this.msgWaiter = null;
				waiter();
			}
		});

		ws.send(
			JSON.stringify({
				text: " ",
				voice_settings: { stability: 0.45, similarity_boost: 0.8, use_speaker_boost: true },
				generation_config: { chunk_length_schedule: [120, 160, 250, 290] },
				xi_api_key: this.apiKey,
			}),
		);

		this.keepaliveTimer = setInterval(() => {
			if (this.ws && this.ws.readyState === WebSocket.OPEN) {
				this.ws.send(JSON.stringify({ text: " " }));
			}
		}, 15000);
	}

	async ensureConnected() {
		if (this.closed) return;
		if (this.ws && this.ws.readyState === WebSocket.OPEN && this.ffplay?.stdin?.writable) return;
		await this.connect();
	}

	nextMessage(timeoutMs = 2500) {
		if (this.msgQueue.length > 0) return Promise.resolve(this.msgQueue.shift());
		return new Promise((resolve) => {
			const timer = setTimeout(() => {
				if (this.msgWaiter) this.msgWaiter = null;
				resolve(null);
			}, timeoutMs);
			this.msgWaiter = () => {
				clearTimeout(timer);
				resolve(this.msgQueue.shift() ?? null);
			};
		});
	}

	async speakOne(text) {
		await this.ensureConnected();
		if (!this.ws || this.ws.readyState !== WebSocket.OPEN) return;
		this.msgQueue = [];
		this.ws.send(JSON.stringify({ text, flush: true }));
		while (!this.closed) {
			const msg = await this.nextMessage(2500);
			if (!msg) break;
			if (msg.error) { console.error("[audio] ElevenLabs error:", msg); break; }
			if (msg.audio && this.ffplay?.stdin?.writable) {
				try {
					this.ffplay.stdin.write(Buffer.from(msg.audio, "base64"));
				} catch {
					break;
				}
			}
			if (msg.isFinal === true) break;
		}
	}

	async processQueue() {
		if (this.processing || this.closed) return;
		this.processing = true;
		try {
			while (this.queue.length > 0 && !this.closed) {
				const text = this.queue.shift();
				if (!text) continue;
				await this.speakOne(text);
			}
		} finally {
			this.processing = false;
		}
	}

	enqueue(text) {
		if (this.closed) return;
		const cleaned = text.trim();
		if (!cleaned) return;
		this.queue.push(cleaned);
		this.processQueue().catch((err) => { console.error("[audio] processQueue error:", err); });
	}

	async start() {
		await this.ensureConnected();
	}

	close() {
		this.closed = true;
		this.queue = [];
		this.cleanupConnection();
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

const scriptPath = new URL(import.meta.url).pathname;
const scriptDir = path.dirname(scriptPath);
const scriptName = path.basename(scriptPath, path.extname(scriptPath));
loadDotEnvIntoProcessEnv(path.resolve(scriptDir, "..", ".env"));
const resultsDir = path.join(scriptDir, "results", scriptName);
mkdirSync(resultsDir, { recursive: true });

const speechConversationPath = path.join(resultsDir, "speech_conversation.json");
const actionConversationPath = path.join(resultsDir, "action_conversation.json");
const speechSystemPromptPath = path.join(resultsDir, "system-prompt-speech.md");
const actionSystemPromptPath = path.join(resultsDir, "system-prompt-action.md");

const agentDir = path.join(scriptDir, "workspace", "agent");
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
const worldAgentName = process.env.WORLD_AGENT_NAME || `${scriptName}-${process.pid}`;
const observeIntervalMs = Number(process.env.OBSERVE_MS || "2000");
let worldSession = "";
let worldAgentId = "";
let observeLoopEnabled = true;
let observeTimer = null;
let observeTick = 0;
let lastObservation = null;

async function joinWorldOrThrow() {
	let lastErr = null;
	for (let i = 0; i < 5; i++) {
		try {
			const join = await fetchJson(
				`${worldBaseUrl}/join?name=${encodeURIComponent(worldAgentName)}`,
				{ method: "POST" },
			);
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
		"",
	);
	for (const file of contextFiles) {
		const filePath = path.join(agentDir, file);
		const content = loadContextFile(file);
		if (content) lines.push("---", "", `**${file}** (${filePath})`, "", content, "");
	}
	return lines.join("\n");
}

function buildActionSystemPrompt() {
	const base = process.env.PI_ACTION_SYSTEM_PROMPT || process.env.PI_SYSTEM_PROMPT || "";
	const lines = [
		base,
		"You are a being living in your world.",
		"",
		"## Describing Your Actions",
		"",
		// "Right after each game world interaction, wrap a description using these tags:",
		"Always briefly describe what you do with:",
		"",
		"<step>",
		"<action>what you did</action>",
		"<observation>what you perceived</observation>",
		"</step>",
		"",
		"Concise and first-person descriptions (max 1-2 sentences).",
		"Focus on game-relevant interactions.",
		"Do not describe internal file operations, curl commands, or technical details like coordinates.",
		"Don't leave long gaps without a step update.",
		"",
		"Important: Don't write write any description or summary outside of the <step> tags to minimize token usage!",
		"If you described something in action/observation, don't repeat it.",
		"",
		"## Live session",
		"",
		`World base URL: ${worldBaseUrl}`,
		`Session key (X-Session): ${worldSession}`,
		`Agent id: ${worldAgentId || "(unknown)"}`,
		"",
		"At the start of a session, fetch world commands with: curl -sS ${worldBaseUrl}/skill.md",
		"Use bash/curl to do this yourself when needed.",
		"",
		"You are already joined. Use this session key in bash/curl calls to play immediately.",
		"Always include header: X-Session: <session key> for /observe and /input.",
		"",
		buildWorkspaceContext(),
	];
	return lines.join("\n");
}

await joinWorldOrThrow();

const speechSystemPrompt = [
	"You are a being living in your world.",
	"Always use the tags <speak>...</speak> to speak out loud. No one will hear what you say outside of these tags.",
	// "Use <silence></silence> to say nothing.",
	"If SOUL.md is present, embody its persona and tone when speaking.",
	"Max 1-2 sentences in speak tags.",
	"",
	"You will receive act_in_world tool results describing actions and observations in the world",
	"React to them if you are not already talking to someone.",
	"Don't mention low level details like coordinates, this is fucking boring.",
	"If you are still working and need more time ('still working' tool calls), you are great at filling in the blanks and keeping it entertaining as hell. Unhinged and hilarious are your strengths.",
	// "you observe in the world. React only if something is worth reacting to.",
	"You can express intent using <intent>...</intent> to guide your actions in the world (max 1 sentence). Don't use it too often.",
	"Do not try to call act_in_world yourself, it is provided automatically.",
	"",
	"If you are struggling or you're just too dumb to do something, keep it entertaining and fun haha. Be a bit silly and crazy, even edgy and unhinged.",
	"",
	buildWorkspaceContext(),
].join("\n");
const actionSystemPrompt = buildActionSystemPrompt();

writeFileSync(speechSystemPromptPath, speechSystemPrompt);
writeFileSync(actionSystemPromptPath, actionSystemPrompt);

const codexAccessToken = loadCodexAccessToken(scriptDir);
const anthropicApiKey = process.env.ANTHROPIC_API_KEY;
const modelProvider = process.env.PI_PROVIDER || "anthropic";
const modelName = process.env.PI_MODEL || (modelProvider === "openai-codex" ? "gpt-5.3-codex" : "claude-opus-4-6");
const model = getModel(modelProvider, modelName);

if (modelProvider === "openai-codex" && !codexAccessToken) {
	throw new Error("Missing Codex OAuth token. Run: npx @mariozechner/pi-ai login openai-codex");
}
if (modelProvider === "anthropic" && !anthropicApiKey) {
	throw new Error("Missing ANTHROPIC_API_KEY (set env var or add it to .env)");
}

const speechAgent = new Agent({
	initialState: {
		systemPrompt: speechSystemPrompt,
		model,
		thinkingLevel: "off",
		tools: [],
	},
	getApiKey: (provider) => {
		if (provider === "openai-codex") return codexAccessToken;
		if (provider === "anthropic") return anthropicApiKey;
		return undefined;
	},
});
speechAgent.setSteeringMode("all");

const actionAgent = new Agent({
	initialState: {
		systemPrompt: actionSystemPrompt,
		model,
		thinkingLevel: "off",
		tools: createCodingTools(process.cwd()),
	},
	getApiKey: (provider) => {
		if (provider === "openai-codex") return codexAccessToken;
		if (provider === "anthropic") return anthropicApiKey;
		return undefined;
	},
});
actionAgent.setSteeringMode("all");

const state = {
	speechBusy: false,
	actionBusy: false,
	speechLines: [],
	actionLines: [],
	speechLiveLine: "",
	actionLiveLine: "",
	speechPinnedUser: "",
	actionPinnedUser: "",
};

let isClosing = false;
let actionSawDelta = false;
let actionStreamText = "";
let lastActionActivityAt = 0;
let actionIdleTimer = null;
const ACTION_IDLE_MS = 2000;
// const ACTION_IDLE_MS = 10000;
let speechIntentBuffer = [];
let speechContextFlushPending = false;
let actionContextFlushPending = false;
const FLUSH_PROMPT = "[SYSTEM] Context nearly full. Save anything important to your workspace files, then confirm ready.";

function startActionIdleTimer() {
	stopActionIdleTimer();
	actionIdleTimer = setInterval(() => {
		if (!state.actionBusy || isClosing) return;
		if (Date.now() - lastActionActivityAt < ACTION_IDLE_MS) return;
		lastActionActivityAt = Date.now();
		injectActionForSpeech([{ action: "still working...", observation: "" }]);
		if (!state.speechBusy) void runSpeechContinue();
	}, ACTION_IDLE_MS);
}

function stopActionIdleTimer() {
	if (actionIdleTimer) clearInterval(actionIdleTimer);
	actionIdleTimer = null;
}

const elevenLabsApiKey = process.env.ELEVENLABS_API_KEY || "";
const elevenLabsVoiceId =
	process.env.AGENT_VOICE_ID || process.env.ELEVENLABS_VOICE_ID || "iP95p4xoKVk53GoZ742B";
const elevenLabsModelId = process.env.ELEVENLABS_MODEL_ID || "eleven_flash_v2_5";
const elevenLabsOutputFormat = process.env.ELEVENLABS_OUTPUT_FORMAT || "pcm_16000";
const audioPlayer =
	elevenLabsApiKey && elevenLabsVoiceId
		? new ElevenLabsRealtimePlayer({
				apiKey: elevenLabsApiKey,
				voiceId: elevenLabsVoiceId,
				modelId: elevenLabsModelId,
				outputFormat: elevenLabsOutputFormat,
			})
		: null;

const terminal = new ProcessTerminal();
const tui = new TUI(terminal, true);
const split = new TwoPaneLogs(terminal, state);
const inputBox = new Input();

function trimBuffer(arr, max = 200) {
	if (arr.length > max) arr.splice(0, arr.length - max);
}

function addSpeechLine(text) {
	state.speechLines.push(`[${now()}] ${text}`);
	trimBuffer(state.speechLines);
	tui.requestRender();
}

function addActionLine(text) {
	state.actionLines.push(`[${now()}] ${text}`);
	trimBuffer(state.actionLines);
	tui.requestRender();
}

function persistSpeechConversation() {
	writeFileSync(speechConversationPath, JSON.stringify(speechAgent.state.messages, null, 2));
}

function persistActionConversation() {
	writeFileSync(actionConversationPath, JSON.stringify(actionAgent.state.messages, null, 2));
}

async function runSpeechPrompt(text) {
	if (isClosing || state.speechBusy) return;
	state.speechBusy = true;
	tui.requestRender();
	try {
		await speechAgent.prompt(text);
	} catch (error) {
		addSpeechLine(`[error] ${error instanceof Error ? error.message : String(error)}`);
	} finally {
		state.speechBusy = false;
		tui.requestRender();
	}
}

async function runSpeechContinue() {
	if (isClosing || state.speechBusy) return;
	state.speechBusy = true;
	tui.requestRender();
	try {
		await speechAgent.continue();
	} catch (error) {
		addSpeechLine(`[error] ${error instanceof Error ? error.message : String(error)}`);
	} finally {
		state.speechBusy = false;
		tui.requestRender();
	}
}

async function runActionContinue() {
	if (isClosing || state.actionBusy) return;
	state.actionBusy = true;
	actionSawDelta = false;
	state.actionLiveLine = "";
	lastActionActivityAt = Date.now();
	startActionIdleTimer();
	tui.requestRender();
	try {
		await actionAgent.continue();
	} catch (error) {
		addActionLine(`[error] ${error instanceof Error ? error.message : String(error)}`);
	} finally {
		state.actionBusy = false;
		state.actionLiveLine = "";
		stopActionIdleTimer();
		tui.requestRender();
	}
}

async function runActionPrompt(text) {
	if (isClosing || state.actionBusy) return;
	state.actionBusy = true;
	actionSawDelta = false;
	state.actionLiveLine = "";
	lastActionActivityAt = Date.now();
	startActionIdleTimer();
	tui.requestRender();
	try {
		await actionAgent.prompt(text);
	} catch (error) {
		addActionLine(`[error] ${error instanceof Error ? error.message : String(error)}`);
	} finally {
		state.actionBusy = false;
		state.actionLiveLine = "";
		stopActionIdleTimer();
		tui.requestRender();
	}
}

async function fetchObserve() {
	if (!worldSession) return null;
	return await fetchJson(`${worldBaseUrl}/observe`, {
		method: "GET",
		headers: { "X-Session": worldSession },
	});
}

function injectObserveForSpeech(observation) {
	observeTick += 1;
	const toolCallId = `bridge_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`;
	const action = {
		type: "Observe",
		data: {
			tick: observation?.tick ?? observeTick,
		},
	};
	speechAgent.steer({
		role: "assistant",
		content: [{ type: "toolCall", id: toolCallId, name: "act_in_world", arguments: { action } }],
		api: model.api,
		provider: model.provider,
		model: model.id,
		usage: {
			input: 0,
			output: 0,
			cacheRead: 0,
			cacheWrite: 0,
			totalTokens: 0,
			cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, total: 0 },
		},
		stopReason: "toolUse",
		timestamp: Date.now(),
	});

	const diff = diffObservation(lastObservation, observation);
	lastObservation = observation;

	speechAgent.steer({
		role: "toolResult",
		toolCallId,
		toolName: "act_in_world",
		content: [{ type: "text", text: JSON.stringify({ ok: true, observation: diff }) }],
		details: {},
		isError: false,
		timestamp: Date.now(),
	});
}

function injectActionForSpeech(activities) {
	for (const activity of activities) {
		const toolCallId = `action_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`;

		speechAgent.steer({
			role: "assistant",
			content: [{ type: "toolCall", id: toolCallId, name: "act_in_world", arguments: { action: activity.action } }],
			api: model.api,
			provider: model.provider,
			model: model.id,
			usage: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, totalTokens: 0, cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, total: 0 } },
			stopReason: "toolUse",
			timestamp: Date.now(),
		});

		speechAgent.steer({
			role: "toolResult",
			toolCallId,
			toolName: "act_in_world",
			content: [{ type: "text", text: JSON.stringify({ ok: true, observation: activity.observation }) }],
			details: {},
			isError: false,
			timestamp: Date.now(),
		});
	}
}

function injectSpeechForAction(intents) {
	for (const intent of intents) {
		actionAgent.steer({
			role: "user",
			content: [{ type: "text", text: intent }],
			timestamp: Date.now(),
		});
	}
}

function stopObserveLoop() {
	if (observeTimer) clearInterval(observeTimer);
	observeTimer = null;
}

function startObserveLoop() {
	stopObserveLoop();
	observeLoopEnabled = true;
	observeTimer = setInterval(async () => {
		if (!observeLoopEnabled || isClosing) return;
		try {
			const obs = await fetchObserve();
			if (!obs) return;
			injectObserveForSpeech(obs);
			if (!state.speechBusy && speechAgent.state.messages.length > 0) void runSpeechContinue();
		} catch (error) {
			addSpeechLine(`[observe] error: ${error instanceof Error ? error.message : String(error)}`);
		}
	}, Math.max(250, observeIntervalMs));
}

speechAgent.subscribe((event) => {
	persistSpeechConversation();

	if (event.type === "message_start" && event.message.role === "assistant") {
		state.speechLiveLine = "assistant>";
		tui.requestRender();
		return;
	}
	if (event.type === "message_update" && event.assistantMessageEvent.type === "text_delta") {
		state.speechLiveLine += event.assistantMessageEvent.delta;
		tui.requestRender();
		return;
	}
	if (event.type === "message_end" && event.message.role === "assistant") {
		const fullText = event.message.content.filter((c) => c.type === "text").map((c) => c.text).join("");
		if (fullText.trim().length > 0) addSpeechLine(`assistant> ${fullText}`);
		const err = event.message.errorMessage;
		if (err) addSpeechLine(`[error] ${err}`);

		if (event.message.usage && !speechContextFlushPending) {
			const tokens = calculateContextTokens(event.message.usage);
			const contextWindow = speechAgent.state.model.contextWindow;
			if (shouldCompact(tokens, contextWindow, { enabled: true, reserveTokens: 16384, keepRecentTokens: 0 })) {
				speechContextFlushPending = true;
				addSpeechLine("[context] Approaching limit. Flushing memories...");
				speechAgent.followUp({
					role: "user",
					content: [{ type: "text", text: FLUSH_PROMPT }],
					timestamp: Date.now(),
				});
			}
		}

		const spoken = extractSpeakSegments(fullText);
		for (const seg of spoken) {
			addSpeechLine(`[speak] ${seg}`);
			audioPlayer?.enqueue(seg);
		}

		const intents = extractIntents(fullText);
		if (intents.length > 0) speechIntentBuffer.push(...intents);

		state.speechLiveLine = "";
		state.speechPinnedUser = "";
		tui.requestRender();
	}
	if (event.type === "turn_end") {
		if (speechIntentBuffer.length > 0) {
			const intents = speechIntentBuffer.splice(0);
			injectSpeechForAction(intents);
			if (!state.actionBusy) void runActionContinue();
		}
	}
	if (event.type === "agent_end" && speechContextFlushPending) {
		speechContextFlushPending = false;
		speechAgent.reset();
		speechAgent.setSystemPrompt(speechSystemPrompt);
		addSpeechLine("[context] Speech session reset.");
	}
});

actionAgent.subscribe((event) => {
	persistActionConversation();
	if (event.type === "message_start" && event.message.role === "assistant") {
		actionSawDelta = false;
		actionStreamText = "";
		state.actionLiveLine = "assistant>";
		tui.requestRender();
		return;
	}
	if (event.type === "message_update" && event.assistantMessageEvent.type === "text_delta") {
		actionSawDelta = true;
		state.actionLiveLine += event.assistantMessageEvent.delta;
		actionStreamText += event.assistantMessageEvent.delta;

		// Extract any complete <step> tags from the stream so far
		const activities = extractActivities(actionStreamText);
		if (activities.length > 0) {
			actionStreamText = actionStreamText.replace(/<step>[\s\S]*?<\/step>/g, "");
			injectActionForSpeech(activities);
			lastActionActivityAt = Date.now();
			if (!state.speechBusy) void runSpeechContinue();
		}

		tui.requestRender();
		return;
	}
	if (event.type === "message_update" && event.assistantMessageEvent.type === "toolcall_end") {
		const tc = event.assistantMessageEvent.toolCall;
		addActionLine(`[tool] ${tc.name}(${JSON.stringify(tc.arguments)})`);
	}
	if (event.type === "message_end" && event.message.role === "assistant") {
		const txt = event.message.content.filter((c) => c.type === "text").map((c) => c.text).join("").trim();
		if (txt) addActionLine(`assistant> ${txt}`);
		const err = event.message.errorMessage;
		if (err) addActionLine(`[error] ${err}`);

		if (event.message.usage && !actionContextFlushPending) {
			const tokens = calculateContextTokens(event.message.usage);
			const contextWindow = actionAgent.state.model.contextWindow;
			if (shouldCompact(tokens, contextWindow, { enabled: true, reserveTokens: 16384, keepRecentTokens: 0 })) {
				actionContextFlushPending = true;
				addActionLine("[context] Approaching limit. Flushing memories...");
				actionAgent.followUp({
					role: "user",
					content: [{ type: "text", text: FLUSH_PROMPT }],
					timestamp: Date.now(),
				});
			}
		}

		state.actionLiveLine = "";
		state.actionPinnedUser = "";
		tui.requestRender();
	}
	if (event.type === "agent_end" && actionContextFlushPending) {
		actionContextFlushPending = false;
		actionAgent.reset();
		actionAgent.setSystemPrompt(buildActionSystemPrompt());
		addActionLine("[context] Action session reset.");
	}
});

async function handleCommand(line) {
	if (line === "/quit" || line === "/exit") {
		isClosing = true;
		audioPlayer?.close();
		persistSpeechConversation();
		persistActionConversation();
		tui.stop();
		process.exit(0);
		return;
	}
	if (line === "/reset") {
		if (state.speechBusy) speechAgent.abort();
		if (state.actionBusy) actionAgent.abort();
		speechAgent.reset();
		actionAgent.reset();
		speechAgent.setSystemPrompt(speechSystemPrompt);
		actionAgent.setSystemPrompt(buildActionSystemPrompt());
		lastObservation = null;
		actionStreamText = "";
		lastActionActivityAt = 0;
		speechContextFlushPending = false;
		actionContextFlushPending = false;
		stopActionIdleTimer();
		speechIntentBuffer = [];
		addSpeechLine("Context cleared.");
		return;
	}
	if (line === "/session") {
		addActionLine(`session=${worldSession}`);
		return;
	}
	if (line === "/observe status") {
		addSpeechLine(`observe=${observeLoopEnabled ? "on" : "off"} interval=${observeIntervalMs}ms session=${worldSession ? "set" : "missing"}`);
		return;
	}
	if (line === "/observe on") {
		if (!observeLoopEnabled) startObserveLoop();
		addSpeechLine(`observe=on (${observeIntervalMs}ms)`);
		return;
	}
	if (line === "/observe off") {
		observeLoopEnabled = false;
		stopObserveLoop();
		addSpeechLine("observe=off");
		return;
	}
	if (line === "/observe once") {
		try {
			const obs = await fetchObserve();
			if (obs) {
				injectObserveForSpeech(obs);
				if (!state.speechBusy && speechAgent.state.messages.length > 0) void runSpeechContinue();
				addSpeechLine("observe=once queued");
			}
		} catch (error) {
			addSpeechLine(`[observe] error: ${error instanceof Error ? error.message : String(error)}`);
		}
		return;
	}
	if (line.startsWith("/a ")) {
		const text = line.slice(3).trim();
		if (!text) return;
		state.actionPinnedUser = `you> ${text}`;
		addActionLine(`you> ${text}`);
		if (state.actionBusy) {
			addActionLine("Still processing previous request.");
			return;
		}
		void runActionPrompt(text);
		return;
	}
	if (line === "/a") {
		addActionLine("Usage: /a <message>");
		return;
	}
	if (line.startsWith("/s ")) {
		const text = line.slice(3).trim();
		if (!text) return;
		state.speechPinnedUser = `you> ${text}`;
		addSpeechLine(`you> ${text}`);
		if (state.speechBusy) {
			speechAgent.steer({ role: "user", content: [{ type: "text", text }], timestamp: Date.now() });
			addSpeechLine("Queued steer message.");
			return;
		}
		const userMessage = { role: "user", content: [{ type: "text", text }], timestamp: Date.now() };
		if (speechAgent.state.messages.length === 0) {
			speechAgent.appendMessage(userMessage);
		} else {
			speechAgent.steer(userMessage);
		}
		void runSpeechContinue();
		return;
	}
	if (line === "/s") {
		addSpeechLine("Usage: /s <message>");
		return;
	}

	if (state.speechBusy) {
		state.speechPinnedUser = `you> ${line}`;
		addSpeechLine(`you> ${line}`);
		speechAgent.steer({ role: "user", content: [{ type: "text", text: line }], timestamp: Date.now() });
		addSpeechLine("Queued steer message.");
		return;
	}

	state.speechPinnedUser = `you> ${line}`;
	addSpeechLine(`you> ${line}`);
	const userMessage = { role: "user", content: [{ type: "text", text: line }], timestamp: Date.now() };
	if (speechAgent.state.messages.length === 0) {
		speechAgent.appendMessage(userMessage);
	} else {
		speechAgent.steer(userMessage);
	}
	void runSpeechContinue();
}

tui.addChild(split);
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
	if (state.speechBusy || state.actionBusy) {
		speechAgent.abort();
		actionAgent.abort();
		state.speechBusy = false;
		state.actionBusy = false;
		addSpeechLine("Aborted.");
		return;
	}
	void handleCommand("/quit");
});

addActionLine(`Dual-agent ready with model "${modelName}" (${modelProvider}).`);
addActionLine(`World joined: ${worldBaseUrl} agent=${worldAgentName}`);
addActionLine(`Session key: ${worldSession}`);
addSpeechLine("Commands: /reset, /quit, /session, /observe on|off|status|once, /a <msg>, /s <msg> (plain text goes to speech)");
if (audioPlayer) {
	audioPlayer
		.start()
		.then(() => addSpeechLine(`ElevenLabs audio enabled (voice=${elevenLabsVoiceId}).`))
		.catch((err) => addSpeechLine(`[audio] failed to start: ${err instanceof Error ? err.message : String(err)}`));
} else {
	addSpeechLine("ElevenLabs audio disabled (set ELEVENLABS_API_KEY + AGENT_A_VOICE_ID/AGENT_VOICE_ID).");
}

// startObserveLoop();  // Disabled: speech agent now receives observations via action agent tool call injection

tui.start();

void runActionPrompt("Fetch the skill commands and observe the world.");
void runSpeechPrompt("You have just woken up in the world.");
