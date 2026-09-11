// Called by the isolated Python harness; launch secret arrives only over stdin.
const assert = require('node:assert/strict');
const net = require('node:net');
const path = require('node:path');
const { verifyBackendReady } = require('../desktop/backend-auth.cjs');
const { startServer } = require('../desktop/server.cjs');

function websocketUpgrade(url) {
  const target = new URL(url);
  return new Promise((resolve, reject) => {
    const socket = net.connect({ host: target.hostname, port: Number(target.port) });
    let received = '';
    let settled = false;
    const finish = (error, upgraded = false) => {
      if (settled) return;
      settled = true;
      socket.destroy();
      if (error) reject(error); else resolve(upgraded);
    };
    socket.setTimeout(5000, () => finish(new Error('Proxy WebSocket timed out')));
    socket.on('error', error => finish(error));
    socket.on('close', () => finish(null, false));
    socket.on('data', chunk => {
      received += chunk.toString();
      if (received.includes('\r\n\r\n')) finish(null, received.startsWith('HTTP/1.1 101'));
    });
    socket.on('connect', () => socket.write(
      `GET /api/events/ws HTTP/1.1\r\nHost: ${target.host}\r\nOrigin: ${target.origin}\r\n`
      + 'Connection: Upgrade\r\nUpgrade: websocket\r\nSec-WebSocket-Version: 13\r\n'
      + 'Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\r\n'));
  });
}

async function main() {
  let input = '';
  for await (const chunk of process.stdin) {
    input += chunk;
    if (input.length > 2048) throw new Error('Unexpected smoke-test input size');
  }
  const { secret, version, port, ui_port } = JSON.parse(input);
  assert.equal(await verifyBackendReady(secret, version, { port }), true);
  assert.equal(await verifyBackendReady('ff'.repeat(32), version, { port }), false);
  assert.equal(await verifyBackendReady(secret, 'not-the-running-version', { port }), false);
  let ready = false;
  const { server, url } = await startServer(path.resolve(__dirname, '../frontend/out'), port, ui_port,
    { isBackendReady: () => ready });
  const connections = new Set();
  server.on('connection', socket => {
    connections.add(socket);
    socket.on('close', () => connections.delete(socket));
  });
  try {
    assert.equal((await fetch(`${url}/api/health`)).status, 503);
    assert.equal(await websocketUpgrade(url), false);
    ready = await verifyBackendReady(secret, version, { port });
    assert.equal(ready, true);
    const health = await fetch(`${url}/api/health`, { headers: { Origin: url } });
    assert.equal(health.status, 200);
    assert.equal((await health.json()).version, version);
    assert.equal((await fetch(`${url}/api/chat/cancel-all`, {
      method: 'POST', headers: { Origin: url },
    })).status, 200);
    assert.equal((await fetch(`${url}/api/chat/cancel-all`, {
      method: 'POST', headers: { Origin: 'https://evil.example' },
    })).status, 403);
    assert.equal(await websocketUpgrade(url), true);
    ready = false;
    assert.equal((await fetch(`${url}/api/chat/cancel-all`, { method: 'POST' })).status, 503);
    assert.equal(await websocketUpgrade(url), false);
  } finally {
    // closeAllConnections deliberately excludes upgraded WebSockets.
    for (const socket of connections) socket.destroy();
    server.closeAllConnections();
    await new Promise(resolve => server.close(resolve));
  }
  process.stdout.write('PASS real desktop verifier and HTTP/WebSocket proxy against isolated Rust backend.\n');
}

main().catch(error => {
  // Do not dump assertion input: it can contain the ephemeral launch secret.
  // Retain only our source line/column so a failed assertion can be located.
  const site = /at .*api_security_proxy_smoke\.cjs:(\d+):(\d+)/.exec(String(error.stack));
  const location = site ? ` at api_security_proxy_smoke.cjs:${site[1]}:${site[2]}` : '';
  process.stderr.write(`Desktop/API integration smoke failed: ${error.name}${location}\n`);
  process.exitCode = 1;
});
