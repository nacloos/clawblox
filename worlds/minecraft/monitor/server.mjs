import Fastify from "fastify";
import fastifyProxy from "@fastify/http-proxy";
import fastifyStatic from "@fastify/static";
import websocket from "@fastify/websocket";
import { createReadStream } from "fs";
import { readdir, readFile, stat } from "fs/promises";
import { execFile } from "child_process";
import path from "path";
import os from "os";
import { fileURLToPath } from "url";
import { promisify } from "util";
import pty from "node-pty";

const __dirname = fileURLToPath(new URL(".", import.meta.url));

const PORT = parseInt(process.env.MONITOR_PORT || process.env.PORT || "4310", 10);
const HOST = process.env.MONITOR_HOST || "0.0.0.0";
const POLL_MS = Math.max(250, parseInt(process.env.POLL_MS || "1000", 10));
const VIEWER_URL = process.env.VIEWER_URL || "http://127.0.0.1:3007/";
const VIEWER_UPSTREAM = VIEWER_URL.replace(/\/+$/, "");
const DIST_DIR = path.join(__dirname, "dist");
const PI_SESSIONS_DIR = resolveHomePath(
  process.env.PI_SESSIONS_DIR || "~/.pi/agent/sessions",
);
const AGENT_WORKSPACE_ROOT = resolveHomePath(
  process.env.AGENT_WORKSPACE_ROOT || "/storage/nacloos/projects/clawblox/agent/workspace",
);
const execFileAsync = promisify(execFile);

const app = Fastify({ logger: true });
await app.register(websocket);
await app.register(fastifyStatic, {
  root: path.join(DIST_DIR, "assets"),
  prefix: "/assets/",
  decorateReply: false,
});
await app.register(fastifyProxy, {
  upstream: VIEWER_UPSTREAM,
  prefix: "/viewer",
  rewritePrefix: "",
  websocket: false,
});

function resolveHomePath(p) {
  if (p === "~") return os.homedir();
  if (p.startsWith("~/")) return path.join(os.homedir(), p.slice(2));
  return p;
}

function agentFromDirName(dirName) {
  const workspaceIdx = dirName.lastIndexOf("workspace-");
  if (workspaceIdx === -1) return dirName;
  const rest = dirName.slice(workspaceIdx + "workspace-".length);
  return rest.endsWith("--") ? rest.slice(0, -2) : rest;
}

function safePathWithin(baseDir, maybeChild) {
  const rel = path.relative(baseDir, maybeChild);
  return rel && !rel.startsWith("..") && !path.isAbsolute(rel);
}

async function listLatestSessions() {
  let dirEntries = [];
  try {
    dirEntries = await readdir(PI_SESSIONS_DIR, { withFileTypes: true });
  } catch {
    return [];
  }

  const agents = [];
  for (const entry of dirEntries) {
    if (!entry.isDirectory()) continue;
    const folder = path.join(PI_SESSIONS_DIR, entry.name);
    let children = [];
    try {
      children = await readdir(folder, { withFileTypes: true });
    } catch {
      continue;
    }

    const jsonlFiles = [];
    for (const child of children) {
      if (!child.isFile() || !child.name.endsWith(".jsonl")) continue;
      const fullPath = path.join(folder, child.name);
      try {
        const st = await stat(fullPath);
        jsonlFiles.push({ fullPath, mtimeMs: st.mtimeMs, size: st.size });
      } catch {
        // ignore racing file changes
      }
    }

    if (jsonlFiles.length === 0) continue;
    jsonlFiles.sort((a, b) => b.mtimeMs - a.mtimeMs);
    const latest = jsonlFiles[0];

    agents.push({
      id: agentFromDirName(entry.name),
      folder: entry.name,
      latestFile: latest.fullPath,
      latestFileName: path.basename(latest.fullPath),
      mtimeMs: latest.mtimeMs,
      size: latest.size,
    });
  }

  agents.sort((a, b) => b.mtimeMs - a.mtimeMs);
  return agents;
}

async function resolveAgentFile(agentId) {
  const agents = await listLatestSessions();
  return agents.find((a) => a.id === agentId) || null;
}

function resolveAgentWorkspace(agentId) {
  return path.join(AGENT_WORKSPACE_ROOT, agentId);
}

function resolveWorkspacePath(agentId, relPath = "") {
  const base = resolveAgentWorkspace(agentId);
  const candidate = path.resolve(base, relPath || ".");
  const rel = path.relative(base, candidate);
  if (rel.startsWith("..") || path.isAbsolute(rel)) return null;
  return { base, candidate, relPath: rel === "" ? "." : rel };
}

async function listTmuxSessions() {
  try {
    const { stdout } = await execFileAsync("tmux", [
      "list-panes",
      "-a",
      "-F",
      "#{session_name}\t#{pane_title}\t#{pane_current_command}\t#{pane_width}\t#{pane_height}",
    ]);
    const map = new Map();
    for (const line of stdout.split("\n").map((s) => s.trim()).filter(Boolean)) {
      const [name, title = "", command = "", width = "0", height = "0"] = line.split("\t");
      if (!name || map.has(name)) continue;
      map.set(name, {
        name,
        title,
        command,
        width: Number.parseInt(width, 10) || 0,
        height: Number.parseInt(height, 10) || 0,
      });
    }
    return [...map.values()];
  } catch {
    return [];
  }
}

function parseJsonLine(line) {
  try {
    const raw = JSON.parse(line);
    return {
      type: "event",
      timestamp: raw.timestamp || null,
      eventType: raw.type || "unknown",
      raw,
    };
  } catch {
    return {
      type: "raw_line",
      timestamp: null,
      eventType: "raw_line",
      raw: line,
    };
  }
}

async function readLastLines(filePath, limit = 200) {
  try {
    const text = await readFile(filePath, "utf8");
    const lines = text.split("\n").filter(Boolean);
    return lines.slice(-Math.max(1, Math.min(limit, 2000))).map(parseJsonLine);
  } catch {
    return [];
  }
}

function readChunk(filePath, start, endInclusive) {
  return new Promise((resolve, reject) => {
    const chunks = [];
    const rs = createReadStream(filePath, {
      encoding: "utf8",
      start,
      end: endInclusive,
    });
    rs.on("data", (c) => chunks.push(c));
    rs.on("error", reject);
    rs.on("end", () => resolve(chunks.join("")));
  });
}

app.get("/healthz", async () => ({ ok: true }));

app.get("/api/config", async () => ({
  viewerUrl: "/viewer/",
  viewerUpstream: VIEWER_URL,
  pollMs: POLL_MS,
  sessionsDir: PI_SESSIONS_DIR,
}));

app.get("/api/viewer/health", async (_request, reply) => {
  try {
    const res = await fetch(VIEWER_UPSTREAM, {
      method: "GET",
      signal: AbortSignal.timeout(2000),
    });
    return {
      ok: res.ok,
      status: res.status,
      statusText: res.statusText,
      upstream: VIEWER_URL,
    };
  } catch (err) {
    return reply.code(503).send({
      ok: false,
      upstream: VIEWER_URL,
      error: err instanceof Error ? err.message : String(err),
    });
  }
});

app.get("/api/agents", async () => {
  const agents = await listLatestSessions();
  return {
    agents: agents.map((a) => ({
      id: a.id,
      latestFile: a.latestFileName,
      mtimeMs: a.mtimeMs,
      size: a.size,
      workspace: resolveAgentWorkspace(a.id),
    })),
  };
});

app.get("/api/tmux/sessions", async () => {
  const sessions = await listTmuxSessions();
  return { sessions };
});

app.get("/api/events", async (request, reply) => {
  const q = request.query || {};
  const agent = typeof q.agent === "string" ? q.agent : "";
  const limit = Number.isFinite(Number(q.limit)) ? Number(q.limit) : 200;
  if (!agent) {
    return reply.code(400).send({ error: "Missing agent query param" });
  }

  const hit = await resolveAgentFile(agent);
  if (!hit) {
    return reply.code(404).send({ error: `Unknown agent: ${agent}` });
  }

  const events = await readLastLines(hit.latestFile, limit);
  return { agent, file: hit.latestFileName, events };
});

app.get("/api/workspace/list", async (request, reply) => {
  const q = request.query || {};
  const agent = typeof q.agent === "string" ? q.agent : "";
  const relPath = typeof q.path === "string" ? q.path : ".";
  if (!agent) {
    return reply.code(400).send({ error: "Missing agent query param" });
  }

  const resolved = resolveWorkspacePath(agent, relPath);
  if (!resolved) {
    return reply.code(400).send({ error: "Invalid workspace path" });
  }

  let entries = [];
  try {
    entries = await readdir(resolved.candidate, { withFileTypes: true });
  } catch (err) {
    return reply.code(404).send({
      error: err instanceof Error ? err.message : "Workspace directory not found",
    });
  }

  const out = [];
  for (const entry of entries) {
    const full = path.join(resolved.candidate, entry.name);
    let st;
    try {
      st = await stat(full);
    } catch {
      continue;
    }
    out.push({
      name: entry.name,
      type: entry.isDirectory() ? "dir" : "file",
      size: st.size,
      mtimeMs: st.mtimeMs,
    });
  }

  out.sort((a, b) => {
    if (a.type !== b.type) return a.type === "dir" ? -1 : 1;
    return a.name.localeCompare(b.name);
  });

  return {
    agent,
    root: resolved.base,
    path: resolved.relPath,
    entries: out,
  };
});

app.get("/api/workspace/file", async (request, reply) => {
  const q = request.query || {};
  const agent = typeof q.agent === "string" ? q.agent : "";
  const relPath = typeof q.path === "string" ? q.path : "";
  if (!agent || !relPath) {
    return reply.code(400).send({ error: "Missing agent or path query param" });
  }

  const resolved = resolveWorkspacePath(agent, relPath);
  if (!resolved) {
    return reply.code(400).send({ error: "Invalid workspace path" });
  }

  let st;
  try {
    st = await stat(resolved.candidate);
  } catch (err) {
    return reply.code(404).send({
      error: err instanceof Error ? err.message : "Workspace file not found",
    });
  }

  if (!st.isFile()) {
    return reply.code(400).send({ error: "Path is not a file" });
  }

  const limit = 512 * 1024;
  const content = await readFile(resolved.candidate, "utf8");
  const truncated = content.length > limit;
  return {
    agent,
    path: resolved.relPath,
    size: st.size,
    truncated,
    content: truncated ? content.slice(0, limit) : content,
  };
});

app.get("/", async (_request, reply) => {
  try {
    const html = await readFile(path.join(DIST_DIR, "index.html"), "utf8");
    reply.type("text/html").send(html);
  } catch {
    reply
      .code(503)
      .type("text/plain")
      .send(
        "UI build missing. Run `npm run build` in worlds/minecraft/monitor, then restart.",
      );
  }
});

app.get("/api/term", { websocket: true }, async (socket, request) => {
  const q = request.query || {};
  const requestedSession =
    typeof q.session === "string" && q.session.length > 0 ? q.session : "";

  let session = requestedSession;
  if (!session) {
    const sessions = await listTmuxSessions();
    session = sessions[0]?.name || "";
  }

  if (!session) {
    socket.send(JSON.stringify({ type: "error", message: "No tmux session available" }));
    socket.close();
    return;
  }

  const sessions = await listTmuxSessions();
  const chosen = sessions.find((s) => s.name === session);
  const defaultCols = chosen?.width || 140;
  const defaultRows = chosen?.height || 42;

  const cols = Number.isFinite(Number(q.cols))
    ? Math.max(40, Number(q.cols))
    : Math.max(40, defaultCols);
  const rows = Number.isFinite(Number(q.rows))
    ? Math.max(12, Number(q.rows))
    : Math.max(12, defaultRows);

  const proc = pty.spawn("tmux", ["attach-session", "-r", "-t", session], {
    name: "xterm-256color",
    cols,
    rows,
    cwd: process.cwd(),
    env: {
      ...process.env,
      TERM: "xterm-256color",
      COLORTERM: "truecolor",
      LANG: process.env.LANG || "en_US.UTF-8",
      LC_ALL: process.env.LC_ALL || "en_US.UTF-8",
    },
  });

  socket.send(JSON.stringify({ type: "meta", session }));

  proc.onData((data) => {
    if (socket.readyState === 1) socket.send(data);
  });

  proc.onExit((evt) => {
    if (socket.readyState === 1) {
      socket.send(`\r\n\r\n[tmux exited: ${evt.exitCode}]\r\n`);
      socket.close();
    }
  });

  socket.on("message", (raw) => {
    let msg;
    try {
      msg = JSON.parse(String(raw));
    } catch {
      return;
    }

    if (msg?.type === "resize") {
      const c = Number(msg.cols);
      const r = Number(msg.rows);
      if (Number.isFinite(c) && Number.isFinite(r)) {
        proc.resize(Math.max(20, c), Math.max(8, r));
      }
    }
  });

  socket.on("close", () => {
    try {
      proc.kill();
    } catch {}
  });
});

app.get("/api/stream", { websocket: true }, (socket, request) => {
  const q = request.query || {};
  const agent = typeof q.agent === "string" ? q.agent : "";
  if (!agent) {
    socket.send(
      JSON.stringify({ type: "error", message: "Missing agent query param" }),
    );
    socket.close();
    return;
  }

  let closed = false;
  let lastFile = "";
  let offset = 0;
  let bufferedTail = "";
  let interval = null;

  async function tick() {
    if (closed) return;

    const hit = await resolveAgentFile(agent);
    if (!hit) {
      socket.send(
        JSON.stringify({
          type: "status",
          status: "waiting",
          message: `No sessions found for ${agent}`,
        }),
      );
      return;
    }

    const filePath = hit.latestFile;
    if (!safePathWithin(PI_SESSIONS_DIR, filePath)) {
      socket.send(
        JSON.stringify({
          type: "error",
          message: "Resolved file path outside sessions dir",
        }),
      );
      return;
    }

    let st;
    try {
      st = await stat(filePath);
    } catch {
      return;
    }

    if (filePath !== lastFile) {
      lastFile = filePath;
      offset = st.size;
      bufferedTail = "";
      socket.send(
        JSON.stringify({
          type: "status",
          status: "switched",
          agent,
          file: path.basename(filePath),
        }),
      );
      return;
    }

    if (st.size < offset) {
      offset = 0;
      bufferedTail = "";
      socket.send(
        JSON.stringify({
          type: "status",
          status: "rotated",
          agent,
          file: path.basename(filePath),
        }),
      );
    }

    if (st.size === offset) return;

    let chunk = "";
    try {
      chunk = await readChunk(filePath, offset, st.size - 1);
    } catch {
      return;
    }
    offset = st.size;

    const combined = bufferedTail + chunk;
    const lines = combined.split("\n");
    bufferedTail = lines.pop() || "";

    for (const line of lines) {
      if (!line) continue;
      const evt = parseJsonLine(line);
      socket.send(JSON.stringify({ type: "event", agent, event: evt }));
    }
  }

  interval = setInterval(() => {
    tick().catch((err) => {
      if (!closed) {
        socket.send(
          JSON.stringify({
            type: "error",
            message: err instanceof Error ? err.message : String(err),
          }),
        );
      }
    });
  }, POLL_MS);

  tick().catch(() => {});

  socket.on("close", () => {
    closed = true;
    if (interval) clearInterval(interval);
  });
});

await app.listen({ host: HOST, port: PORT });
app.log.info(`Monitor UI: http://${HOST}:${PORT}`);
app.log.info(`Sessions: ${PI_SESSIONS_DIR}`);
app.log.info(`Viewer: ${VIEWER_URL}`);
