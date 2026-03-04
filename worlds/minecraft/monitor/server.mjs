import Fastify from "fastify";
import websocket from "@fastify/websocket";
import pty from "node-pty";
import { readFile } from "fs/promises";
import { execFile } from "child_process";
import { promisify } from "util";
import { join } from "path";
import { fileURLToPath } from "url";

const execFileAsync = promisify(execFile);
const __dirname = fileURLToPath(new URL(".", import.meta.url));

const PORT = parseInt(process.env.MONITOR_PORT || process.env.PORT || "3000", 10);
const HOST = process.env.MONITOR_HOST || "127.0.0.1";
const DEFAULT_ADAPTER_WS = process.env.ADAPTER_WS_URL || "ws://127.0.0.1:8081/ws";
const DEFAULT_ADAPTER_HTTP = process.env.ADAPTER_HTTP_URL || "http://127.0.0.1:8081";
const VIEWER_URL = process.env.VIEWER_URL || "http://127.0.0.1:3007/";

const app = Fastify({ logger: true });
await app.register(websocket);

async function listTmuxSessions() {
  try {
    const { stdout } = await execFileAsync("tmux", ["ls"]);
    return stdout
      .split("\n")
      .map((line) => line.trim())
      .filter(Boolean)
      .map((line) => line.split(":")[0]);
  } catch {
    return [];
  }
}

app.get("/api/config", async () => ({
  defaultAdapterWs: DEFAULT_ADAPTER_WS,
  defaultAdapterHttp: DEFAULT_ADAPTER_HTTP,
  viewerUrl: VIEWER_URL,
}));

app.get("/api/tmux/sessions", async () => {
  const sessions = await listTmuxSessions();
  return { sessions };
});

app.get("/", async (_request, reply) => {
  const html = await readFile(join(__dirname, "public/index.html"), "utf8");
  reply.type("text/html").send(html);
});

app.get("/term", { websocket: true }, (socket, request) => {
  const q = request.query || {};
  const session = typeof q.session === "string" && q.session.length > 0 ? q.session : "mc-agent";
  const readonly = String(q.readonly || "1") !== "0";
  const cols = Number.isFinite(Number(q.cols)) ? Math.max(40, Number(q.cols)) : 140;
  const rows = Number.isFinite(Number(q.rows)) ? Math.max(10, Number(q.rows)) : 40;

  const proc = pty.spawn("tmux", ["attach", "-t", session], {
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

  socket.send(JSON.stringify({ type: "meta", data: { session, readonly } }));

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
      return;
    }

    if (readonly) return;

    if (msg?.type === "input" && typeof msg.data === "string") {
      proc.write(msg.data);
    }
  });

  socket.on("close", () => {
    try {
      proc.kill();
    } catch {}
  });
});

await app.listen({ host: HOST, port: PORT });
app.log.info(`Monitor UI: http://${HOST}:${PORT}`);
app.log.info(`Defaults -> adapter ws: ${DEFAULT_ADAPTER_WS}, adapter http: ${DEFAULT_ADAPTER_HTTP}`);
