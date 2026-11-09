const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const path = require('node:path');
const src = fs.readFileSync(path.join(__dirname, '../src/main.js'), 'utf8');
const functions = src.slice(src.indexOf('async function login()'), src.indexOf('function makeFrame'));
function context(fetch) {
  const state = { fetch, AbortController, token: 'test-token', tokenInput: { value: 'test-token' },
    loginController: null, reconnectTimer: null, connectionGeneration: 0, setTimeout: () => 1, clearTimeout: () => {}, tr: (key) => key,
    setStatus: () => {}, hideAuth: () => {}, auth: 0, retry: 0, connections: 0 };
  state.showAuth = () => state.auth++;
  state.scheduleReconnect = () => state.retry++;
  state.connect = () => state.connections++;
  vm.createContext(state); vm.runInContext(functions, state); return state;
}
test('offline retries without asking for credentials', async () => {
  const s = context(async () => { throw new TypeError('offline'); });
  await s.authenticateAndConnect(); assert.equal(s.auth, 0); assert.equal(s.retry, 1);
  assert.equal(s.token, 'test-token');
});
test('immediate reconnect cancels a pending backoff timer before login', async () => {
  const s = context(async () => ({ ok: true }));
  s.reconnectTimer = 42;
  const cancelled = [];
  s.clearTimeout = id => cancelled.push(id);
  await s.authenticateAndConnect();
  assert.ok(cancelled.includes(42)); assert.equal(s.reconnectTimer, null);
  assert.equal(s.connections, 1);
});
for (const status of [429, 500, 503]) test(`${status} retries`, async () => {
  const s = context(async () => ({ ok: false, status }));
  await s.authenticateAndConnect(); assert.equal(s.retry, 1); assert.equal(s.auth, 0);
});
test('401 requests credentials', async () => {
  const s = context(async () => ({ ok: false, status: 401 }));
  await s.authenticateAndConnect(); assert.equal(s.auth, 1); assert.equal(s.retry, 0);
});
test('successful exchange erases bootstrap token', async () => {
  const s = context(async () => ({ ok: true }));
  await s.authenticateAndConnect(); assert.equal(s.token, null); assert.equal(s.tokenInput.value, '');
  assert.equal(s.connections, 1);
});
test('late login failure cannot replace a newer successful connection', async () => {
  const pending = []; const s = context(() => new Promise(resolve => pending.push(resolve)));
  const old = s.authenticateAndConnect(); const current = s.authenticateAndConnect();
  pending[1]({ ok: true }); await current;
  pending[0]({ ok: false, status: 401 }); await old;
  assert.equal(s.connections, 1); assert.equal(s.auth, 0); assert.equal(s.retry, 0);
});
test('late successful login cannot erase a newer credential', async () => {
  const pending = []; const s = context(() => new Promise(resolve => pending.push(resolve)));
  const old = s.authenticateAndConnect();
  s.token = s.tokenInput.value = 'new-credential';
  const current = s.authenticateAndConnect();
  pending[1]({ ok: false, status: 401 }); await current;
  pending[0]({ ok: true }); await old;
  assert.equal(s.token, 'new-credential'); assert.equal(s.tokenInput.value, 'new-credential');
  assert.equal(s.connections, 0);
});
