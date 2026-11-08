import "./style.css";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { WebLinksAddon } from "@xterm/addon-web-links";
import { SearchAddon } from "@xterm/addon-search";
import "@xterm/xterm/css/xterm.css";
import { tr, isZh } from "./i18n";

const termEl = document.getElementById("terminal");
const statusEl = document.getElementById("status");
const modal = document.getElementById("auth-modal");
const form = document.getElementById("auth-form");
const tokenInput = document.getElementById("token");
const authError = document.getElementById("auth-error");
const authTitle = document.getElementById("auth-title");
const authLabel = document.getElementById("auth-label");
const authSubmit = document.getElementById("auth-submit");
const badgeEl = document.getElementById("readonly-badge");
const reconnectBtn = document.getElementById("btn-reconnect");
const keyBar = document.getElementById("mobile-keys");
const keysBtn = document.getElementById("btn-keys");
const themeBtn = document.getElementById("btn-theme");
const fontDecBtn = document.getElementById("btn-font-dec");
const fontIncBtn = document.getElementById("btn-font-inc");
const searchBar = document.getElementById("search-bar");
const searchInput = document.getElementById("search-input");
const searchNext = document.getElementById("search-next");
const searchPrev = document.getElementById("search-prev");
const searchClose = document.getElementById("search-close");
const ctxMenu = document.getElementById("ctx-menu");
const ctxCopy = document.getElementById("ctx-copy");
const ctxPaste = document.getElementById("ctx-paste");
const ctxSelectAll = document.getElementById("ctx-select-all");

const CMD_INPUT = 0x30;
const CMD_RESIZE = 0x31;
const CMD_ATTACH = 0x32;
const CMD_PING = 0x33;
const SRV_OUTPUT = 0x30;
const SRV_SESSION = 0x31;
const SRV_PONG = 0x32;
const SRV_ERROR = 0x33;

const SESSION_KEY = "ttying.session";
const FONT_KEY = "ttying.fontSize";
const THEME_KEY = "ttying.theme";
const KEYS_KEY = "ttying.keysVisible";
const DEFAULT_FONT_SIZE = 14;
const MAX_RECONNECT = 8;
const PING_INTERVAL = 20000;
const RX_TIMEOUT = 45000;

const DARK_THEME = {
  background: "#1e1e1e", foreground: "#d4d4d4", cursor: "#d4d4d4",
  selectionBackground: "rgba(78,201,176,0.28)", black: "#000000",
  red: "#cd3131", green: "#0dbc79", yellow: "#e5e510", blue: "#2472c8",
  magenta: "#bc3fbc", cyan: "#11a8cd", white: "#e5e5e5",
  brightBlack: "#666666", brightRed: "#f14c4c", brightGreen: "#23d18b",
  brightYellow: "#f5f543", brightBlue: "#3b8eea", brightMagenta: "#d670d6",
  brightCyan: "#29b8db", brightWhite: "#e5e5e5",
};
const LIGHT_THEME = {
  background: "#ffffff", foreground: "#1f2937", cursor: "#1f2937",
  selectionBackground: "rgba(37,99,235,0.22)", black: "#000000",
  red: "#c62828", green: "#2e7d32", yellow: "#8a6d00", blue: "#1565c0",
  magenta: "#8e24aa", cyan: "#00838f", white: "#e5e7eb",
  brightBlack: "#6b7280", brightRed: "#d32f2f", brightGreen: "#388e3c",
  brightYellow: "#a67c00", brightBlue: "#1976d2", brightMagenta: "#ab47bc",
  brightCyan: "#0097a7", brightWhite: "#f9fafb",
};

function loadValue(key, fallback) {
  try { return localStorage.getItem(key) ?? fallback; } catch { return fallback; }
}
function saveValue(key, value) {
  try { localStorage.setItem(key, String(value)); } catch { /* best effort */ }
}
function loadFontSize() {
  const n = Number.parseInt(loadValue(FONT_KEY, ""), 10);
  return Number.isInteger(n) ? Math.min(32, Math.max(8, n)) : DEFAULT_FONT_SIZE;
}
function loadTheme() {
  const value = loadValue(THEME_KEY, "auto");
  return ["auto", "dark", "light"].includes(value) ? value : "auto";
}
let fontSize = loadFontSize();
let themeMode = loadTheme();
const term = new Terminal({
  cursorBlink: true,
  fontSize,
  fontFamily: "'Cascadia Mono', Consolas, 'Liberation Mono', monospace",
  scrollback: 10000,
  allowProposedApi: true,
  macOptionIsMeta: true,
});
const fit = new FitAddon();
const search = new SearchAddon();
const webLinks = new WebLinksAddon((_event, uri) => {
  try {
    const url = new URL(uri);
    if (["http:", "https:", "mailto:"].includes(url.protocol)) {
      window.open(url.href, "_blank", "noopener,noreferrer");
    }
  } catch { /* ignore malformed links */ }
});
term.loadAddon(fit);
term.loadAddon(search);
term.loadAddon(webLinks);
term.open(termEl);
term.onTitleChange((title) => {
  document.title = title ? `${title} | ttying` : "ttying";
});

let token = null;
let ws = null;
let reconnectTimer = null;
let reconnectAttempt = 0;
let manualClose = false;
let sessionEnded = false;
let readOnly = false;
let heartbeatTimer = null;
let probeTimer = null;
let lastRxAt = Date.now();
let listenersReady = false;
let attached = false;
let connectionGeneration = 0;
let loginController = null;

function storedSessionId() {
  try { return sessionStorage.getItem(SESSION_KEY); } catch { return null; }
}
function setStoredSessionId(id) {
  try {
    if (id) sessionStorage.setItem(SESSION_KEY, id);
    else sessionStorage.removeItem(SESSION_KEY);
  } catch { /* best effort */ }
}

function resolvedTheme() {
  if (themeMode === "auto") {
    return matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
  }
  return themeMode;
}
function applyTheme() {
  const resolved = resolvedTheme();
  document.documentElement.dataset.theme = resolved;
  term.options.theme = resolved === "dark" ? DARK_THEME : LIGHT_THEME;
  themeBtn.textContent = themeMode === "auto" ? tr("theme_auto") : themeMode === "dark" ? tr("theme_dark") : tr("theme_light");
  themeBtn.title = `Theme: ${themeBtn.textContent}`;
}
function cycleTheme() {
  themeMode = { auto: "dark", dark: "light", light: "auto" }[themeMode];
  saveValue(THEME_KEY, themeMode);
  applyTheme();
  fit.fit();
  sendResize();
}
function setFontSize(next) {
  fontSize = Math.min(32, Math.max(8, next));
  term.options.fontSize = fontSize;
  saveValue(FONT_KEY, fontSize);
  setTimeout(() => { fit.fit(); sendResize(); }, 0);
}
applyTheme();
function setStatus(message, state = "") {
  statusEl.textContent = message;
  statusEl.dataset.state = state;
}
function setReadOnly(value) {
  readOnly = value;
  term.options.disableStdin = value;
  badgeEl.classList.toggle("hidden", !value);
}
function showReconnect(label = tr("reconnect")) {
  reconnectBtn.textContent = label;
  reconnectBtn.classList.remove("hidden");
}
function hideReconnect() {
  reconnectBtn.classList.add("hidden");
}
function showAuth(message = "") {
  modal.classList.remove("hidden");
  authError.textContent = message;
  tokenInput.focus();
}
function hideAuth() {
  modal.classList.add("hidden");
  authError.textContent = "";
}

function parseBootstrapToken() {
  const hash = location.hash;
  if (!hash.startsWith("#auth=")) return null;
  let value;
  try { value = decodeURIComponent(hash.slice(6)); } catch { value = hash.slice(6); }
  history.replaceState(null, "", location.pathname + location.search);
  return value;
}

async function login() {
  const headers = {};
  if (token) headers.Authorization = "Bearer " + token;
  if (loginController) loginController.abort();
  const controller = new AbortController();
  loginController = controller;
  const timer = setTimeout(() => controller.abort(), 10000);
  try {
    const res = await fetch("/api/session", {
      method: "POST", headers, credentials: "same-origin", signal: controller.signal,
    });
    if (res.ok) return "ok";
    return res.status === 401 || res.status === 403 ? "auth" : "retry";
  } catch { return "retry"; }
  finally { clearTimeout(timer); if (loginController === controller) loginController = null; }
}

async function authenticateAndConnect() {
  const generation = ++connectionGeneration;
  const result = await login();
  if (generation !== connectionGeneration) return;
  if (result === "ok") { token = null; tokenInput.value = ""; hideAuth(); connect(); }
  else if (result === "auth") {
    setStatus(tr("auth_required"), "error");
    showAuth(tr("enter_token"));
  } else { scheduleReconnect(); }
}

function makeFrame(kind, payload = new Uint8Array()) {
  const frame = new Uint8Array(payload.length + 1);
  frame[0] = kind;
  frame.set(payload, 1);
  return frame;
}
function sendRaw(kind, payload = new Uint8Array()) {
  if (ws && ws.readyState === WebSocket.OPEN) ws.send(makeFrame(kind, payload));
}
function sendJson(kind, value) {
  sendRaw(kind, new TextEncoder().encode(JSON.stringify(value)));
}
function sendInput(data) {
  if (!attached || readOnly || !ws || ws.readyState !== WebSocket.OPEN) return;
  const bytes = new TextEncoder().encode(data);
  if (bytes.length > 65536 || ws.bufferedAmount > 1024 * 1024) {
    setStatus(tr("input_too_large"), "error");
    return;
  }
  sendRaw(CMD_INPUT, bytes);
}
function sendResize() {
  if (!ws || ws.readyState !== WebSocket.OPEN) return;
  sendJson(CMD_RESIZE, { cols: term.cols, rows: term.rows });
}
function closeSocket() {
  attached = false;
  if (reconnectTimer) {
    clearTimeout(reconnectTimer);
    reconnectTimer = null;
  }
  stopHeartbeat();
  if (!ws) return;
  const old = ws;
  ws = null;
  old.onopen = old.onmessage = old.onerror = old.onclose = null;
  try { old.close(); } catch { /* ignore */ }
}
function stopHeartbeat() {
  if (heartbeatTimer) clearInterval(heartbeatTimer);
  if (probeTimer) clearTimeout(probeTimer);
  heartbeatTimer = null;
  probeTimer = null;
}
function sendPing() {
  if (ws && ws.readyState === WebSocket.OPEN) sendRaw(CMD_PING);
}
function startHeartbeat() {
  stopHeartbeat();
  lastRxAt = Date.now();
  heartbeatTimer = setInterval(() => {
    if (!ws || ws.readyState !== WebSocket.OPEN || document.hidden) return;
    if (Date.now() - lastRxAt > RX_TIMEOUT) {
      ws.close();
      return;
    }
    sendPing();
  }, PING_INTERVAL);
}
function probeConnection() {
  if (!ws || ws.readyState !== WebSocket.OPEN) return;
  const started = Date.now();
  sendPing();
  if (probeTimer) clearTimeout(probeTimer);
  probeTimer = setTimeout(() => {
    if (ws && ws.readyState === WebSocket.OPEN && lastRxAt < started) ws.close();
  }, 3000);
}

function scheduleReconnect() {
  if (manualClose || sessionEnded) return;
  if (modal.classList.contains("hidden") === false) return;
  if (reconnectTimer) return;
  reconnectAttempt += 1;
  if (reconnectAttempt > MAX_RECONNECT) {
    setStatus(tr("disconnected"), "error");
    showReconnect(storedSessionId() ? tr("reconnect") : tr("restart"));
    return;
  }
  const delay = Math.min(1000 * 2 ** (reconnectAttempt - 1), 10000) + Math.floor(Math.random() * 250);
  const seconds = Math.ceil(delay / 1000);
  setStatus(tr("connection_lost", seconds, reconnectAttempt, MAX_RECONNECT), "reconnecting");
  showReconnect(tr("reconnect_now"));
  reconnectTimer = setTimeout(() => {
    reconnectTimer = null;
    void authenticateAndConnect();
  }, delay);
}

function connect() {
  ++connectionGeneration;
  closeSocket();
  manualClose = false;
  sessionEnded = false;
  hideReconnect();
  setStatus(tr("connecting"), "connecting");
  const proto = location.protocol === "https:" ? "wss:" : "ws:";
  ws = new WebSocket(`${proto}//${location.host}/ws`);
  ws.binaryType = "arraybuffer";
  ws.onopen = () => {
    startHeartbeat();
    setStatus(tr("attaching"), "connecting");
    sendJson(CMD_ATTACH, {
      session: storedSessionId(),
      cols: term.cols,
      rows: term.rows,
    });
  };
  ws.onmessage = (event) => {
    if (typeof event.data === "string") return;
    const bytes = new Uint8Array(event.data);
    if (bytes.length === 0) return;
    const kind = bytes[0];
    const payload = bytes.subarray(1);
    lastRxAt = Date.now();
    if (kind === SRV_OUTPUT) {
      term.write(payload);
      return;
    }
    if (kind === SRV_PONG) return;
    if (kind === SRV_SESSION) {
      try {
        const info = JSON.parse(new TextDecoder().decode(payload));
        attached = true;
        reconnectAttempt = 0;
        term.reset();
        setStoredSessionId(info.id);
        setReadOnly(!!info.read_only);
        hideAuth();
        hideReconnect();
        setStatus(info.history_truncated ? tr("partial_history") : (info.reattached ? tr("connected_restored") : tr("connected_new")), "connected");
        setTimeout(() => { fit.fit(); sendResize(); term.focus(); }, 0);
      } catch { /* ignore malformed control message */ }
      return;
    }
    if (kind === SRV_ERROR) {
      try {
        const info = JSON.parse(new TextDecoder().decode(payload));
        if (info.code === "session_ended") {
          sessionEnded = true;
          setStoredSessionId(null);
          setStatus(tr("shell_exited"), "error");
          showReconnect(tr("restart"));
          term.write("\r\n\x1b[31m" + tr("shell_exited_hint") + "\x1b[0m\r\n");
        } else {
          setStatus(tr("error_prefix") + (info.message || info.code), "error");
        }
      } catch { /* ignore malformed control message */ }
    }
  };
  ws.onerror = () => {
    if (!sessionEnded) setStatus(tr("connection_error"), "error");
  };
  ws.onclose = () => {
    stopHeartbeat();
    attached = false;
    ws = null;
    if (manualClose || sessionEnded) return;
    scheduleReconnect();
  };
}
async function copySelection() {
  const selection = term.getSelection();
  if (!selection) {
    setStatus(tr("no_selection"), "");
    return;
  }
  try {
    await navigator.clipboard.writeText(selection);
    setStatus(tr("copied"), "connected");
  } catch {
    try {
      const area = document.createElement("textarea");
      area.value = selection;
      area.style.position = "fixed";
      area.style.opacity = "0";
      document.body.appendChild(area);
      area.select();
      document.execCommand("copy");
      area.remove();
      setStatus(tr("copied"), "connected");
    } catch {
      setStatus(tr("copy_failed"), "error");
    }
  }
}

async function pasteClipboard() {
  if (readOnly) {
    setStatus(tr("read_only"), "error");
    return;
  }
  try {
    const text = await navigator.clipboard.readText();
    if (text) term.paste(text);
  } catch {
    setStatus(tr("paste_unavailable"), "error");
  }
}

function setSearchVisible(visible) {
  searchBar.classList.toggle("hidden", !visible);
  if (visible) {
    searchInput.focus();
    searchInput.select();
  } else {
    search.clearDecorations();
    term.focus();
  }
}
function findNext() {
  if (searchInput.value) search.findNext(searchInput.value, { incremental: true });
}
function findPrevious() {
  if (searchInput.value) search.findPrevious(searchInput.value, { incremental: true });
}
searchInput.addEventListener("input", findNext);
searchNext.addEventListener("click", findNext);
searchPrev.addEventListener("click", findPrevious);
searchClose.addEventListener("click", () => setSearchVisible(false));
searchInput.addEventListener("keydown", (event) => {
  if (event.key === "Enter") {
    event.preventDefault();
    if (event.shiftKey) findPrevious(); else findNext();
  } else if (event.key === "Escape") {
    event.preventDefault();
    setSearchVisible(false);
  }
});

const MOBILE_KEYS = [
  ["Esc", "\x1b"], ["Tab", "\t"], ["Ctrl+C", "\x03"], ["Ctrl+D", "\x04"],
  ["Ctrl+L", "\x0c"], ["↑", "\x1b[A"], ["↓", "\x1b[B"], ["←", "\x1b[D"],
  ["→", "\x1b[C"], ["PgUp", "\x1b[5~"], ["PgDn", "\x1b[6~"], ["Home", "\x1b[H"],
  ["End", "\x1b[F"],
];
for (const [label, sequence] of MOBILE_KEYS) {
  const button = document.createElement("button");
  button.type = "button";
  button.textContent = label;
  button.addEventListener("pointerdown", (event) => event.preventDefault());
  button.addEventListener("click", () => { sendInput(sequence); term.focus(); });
  keyBar.appendChild(button);
}
function setKeyBarVisible(visible) {
  keyBar.classList.toggle("hidden", !visible);
  keysBtn.setAttribute("aria-pressed", String(visible));
  saveValue(KEYS_KEY, String(visible));
  setTimeout(() => { fit.fit(); sendResize(); }, 0);
}
const coarsePointer = matchMedia("(pointer: coarse)").matches;
const storedKeys = loadValue(KEYS_KEY, coarsePointer ? "true" : "false");
setKeyBarVisible(storedKeys === "true");
keysBtn.addEventListener("click", () => setKeyBarVisible(keyBar.classList.contains("hidden")));
themeBtn.addEventListener("click", cycleTheme);
fontDecBtn.addEventListener("click", () => setFontSize(fontSize - 1));
fontIncBtn.addEventListener("click", () => setFontSize(fontSize + 1));
reconnectBtn.addEventListener("click", () => {
  if (sessionEnded) {
    setStoredSessionId(null);
    sessionEnded = false;
  }
  reconnectAttempt = 0;
  closeSocket();
  void authenticateAndConnect();
});
matchMedia("(prefers-color-scheme: dark)").addEventListener("change", () => {
  if (themeMode === "auto") applyTheme();
});
function syncViewportHeight() {
  if (window.visualViewport && visualViewport.height > 0) {
    document.documentElement.style.setProperty("--app-height", `${visualViewport.height}px`);
  }
}

function applyLocale() {
  document.documentElement.lang = isZh ? "zh-CN" : "en";
  searchInput.placeholder = tr("search_placeholder");
  searchPrev.title = tr("search_prev");
  searchNext.title = tr("search_next");
  searchClose.title = tr("search_close");
  authTitle.textContent = tr("auth_title");
  authLabel.textContent = tr("auth_label");
  authSubmit.textContent = tr("auth_submit");
  keysBtn.textContent = tr("keys");
  ctxCopy.textContent = tr("copy");
  ctxPaste.textContent = tr("paste");
  ctxSelectAll.textContent = tr("select_all");
  applyTheme();
  if (statusEl.dataset.state === "") statusEl.textContent = tr("connecting");
}

function setupTerminalListeners() {
  if (listenersReady) return;
  listenersReady = true;
  term.onData((data) => sendInput(data));
  term.onResize(() => sendResize());
  term.attachCustomKeyEventHandler((event) => {
    if (event.type !== "keydown") return true;
    const key = event.key.toLowerCase();
    const mod = event.ctrlKey || event.metaKey;
    const isMac = navigator.platform.toLowerCase().includes("mac");
    if ((mod && event.shiftKey && key === "c") || (isMac && event.metaKey && key === "c")) {
      if (term.hasSelection()) {
        event.preventDefault();
        void copySelection();
        return false;
      }
      return true;
    }
    if ((mod && event.shiftKey && key === "v") || (isMac && event.metaKey && key === "v") || (event.shiftKey && key === "insert")) {
      event.preventDefault();
      void pasteClipboard();
      return false;
    }
    if (mod && event.shiftKey && key === "f") {
      event.preventDefault();
      setSearchVisible(true);
      return false;
    }
    if (mod && ["=", "+"].includes(event.key)) {
      event.preventDefault();
      setFontSize(fontSize + 1);
      return false;
    }
    if (mod && ["-", "_"].includes(event.key)) {
      event.preventDefault();
      setFontSize(fontSize - 1);
      return false;
    }
    if (mod && event.key === "0") {
      event.preventDefault();
      setFontSize(DEFAULT_FONT_SIZE);
      return false;
    }
    return true;
  });
  new ResizeObserver(() => fit.fit()).observe(termEl);
  addEventListener("resize", () => fit.fit());
  syncViewportHeight();
  if (window.visualViewport) {
    visualViewport.addEventListener("resize", () => {
      syncViewportHeight();
      setTimeout(() => { syncViewportHeight(); fit.fit(); sendResize(); }, 50);
    });
    visualViewport.addEventListener("scroll", syncViewportHeight);
  }
  document.fonts.ready.then(() => { fit.fit(); sendResize(); });
}

window.addEventListener("pagehide", () => {
  ++connectionGeneration;
  if (loginController) loginController.abort();
  closeSocket();
});

window.addEventListener("online", () => {
  if (!sessionEnded && !loginController && (!ws || ws.readyState === WebSocket.CLOSED) && modal.classList.contains("hidden")) {
    reconnectAttempt = 0;
    void authenticateAndConnect();
  }
});

document.addEventListener("visibilitychange", () => {
  if (document.hidden) return;
  setTimeout(() => { syncViewportHeight(); fit.fit(); sendResize(); }, 50);
  if (ws && ws.readyState === WebSocket.OPEN) {
    probeConnection();
  } else if ((!ws || ws.readyState === WebSocket.CLOSED) && !loginController && !sessionEnded && modal.classList.contains("hidden")) {
    reconnectAttempt = 0;
    void authenticateAndConnect();
  }
});

window.addEventListener("pageshow", (event) => {
  if (event.persisted) {
    boot().catch((err) => {
      setStatus(tr("reconnect_failed"), "error");
      console.error(err);
    });
  }
});

form.addEventListener("submit", async (event) => {
  event.preventDefault();
  token = tokenInput.value.trim();
  authError.textContent = "";
  if (!token) return;
  await authenticateAndConnect();
});

async function boot() {
  token = parseBootstrapToken();
  closeSocket();
  sessionEnded = false;
  manualClose = false;
  await authenticateAndConnect();
}

async function copyText(text) {
  if (!text) return false;
  try {
    await navigator.clipboard.writeText(text);
    setStatus(tr("copied"), "connected");
    return true;
  } catch {
    try {
      const area = document.createElement("textarea");
      area.value = text;
      area.style.position = "fixed";
      area.style.opacity = "0";
      document.body.appendChild(area);
      area.select();
      document.execCommand("copy");
      area.remove();
      setStatus(tr("copied"), "connected");
      return true;
    } catch {
      setStatus(tr("copy_failed"), "error");
      return false;
    }
  }
}

function hideContextMenu() {
  ctxMenu.classList.add("hidden");
}

termEl.addEventListener("contextmenu", (event) => {
  event.preventDefault();
  hideContextMenu();
  if (term.hasSelection()) {
    const selected = term.getSelection();
    term.clearSelection();
    void copyText(selected);
    return;
  }
  const left = Math.min(event.clientX, window.innerWidth - 160);
  const top = Math.min(event.clientY, window.innerHeight - 130);
  ctxMenu.style.left = `${Math.max(4, left)}px`;
  ctxMenu.style.top = `${Math.max(4, top)}px`;
  ctxMenu.classList.remove("hidden");
});
ctxCopy.addEventListener("click", () => {
  hideContextMenu();
  const selected = term.getSelection();
  if (selected) void copyText(selected); else setStatus(tr("no_selection"), "");
});
ctxPaste.addEventListener("click", () => {
  hideContextMenu();
  void pasteClipboard();
});
ctxSelectAll.addEventListener("click", () => {
  hideContextMenu();
  term.selectAll();
});
document.addEventListener("click", (event) => {
  if (!ctxMenu.contains(event.target)) hideContextMenu();
});
document.addEventListener("keydown", (event) => {
  if (event.key === "Escape") hideContextMenu();
});

applyLocale();
setupTerminalListeners();
boot().catch((err) => {
  setStatus(tr("startup_error"), "error");
  console.error(err);
});
