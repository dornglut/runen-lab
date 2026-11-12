import { test } from 'node:test';
import assert from 'node:assert/strict';
import { once } from 'node:events';
import { createInspectorServer } from '../tools/serve.mjs';

async function runningServer(testContext) {
  const server = createInspectorServer();
  server.listen(0, '127.0.0.1');
  await once(server, 'listening');
  testContext.after(() => server.close());
  return `http://127.0.0.1:${server.address().port}`;
}

test('serves viewer entrypoint and JS assets from loopback', async (t) => {
  const base = await runningServer(t);
  const page = await fetch(base);
  assert.equal(page.status, 200);
  assert.match(page.headers.get('content-type'), /text\/html/);
  assert.match(await page.text(), /graph-svg/);
  const script = await fetch(`${base}/src/graph-data.mjs`);
  assert.equal(script.status, 200);
  assert.match(await script.text(), /validateGraphDocument/);
});

test('rejects unsupported methods, unknown paths, and traversal', async (t) => {
  const base = await runningServer(t);
  assert.equal((await fetch(`${base}/private`)).status, 404);
  assert.equal((await fetch(`${base}/..%2f..%2fCargo.toml`)).status, 404);
  assert.equal((await fetch(`${base}/src/graph-data.mjs`, {method: 'POST'})).status, 405);
});

test('HEAD reports asset metadata without a body', async (t) => {
  const base = await runningServer(t);
  const response = await fetch(`${base}/styles.css`, {method: 'HEAD'});
  assert.equal(response.status, 200);
  assert.ok(Number(response.headers.get('content-length')) > 0);
  assert.equal(await response.text(), '');
});


test('serves the pinned offline D3 distribution and blocks unknown vendor files', async (t) => {
  const base = await runningServer(t);
  const response = await fetch(`${base}/vendor/d3.v7.9.0.min.js`);
  assert.equal(response.status, 200);
  assert.match(response.headers.get('content-type'), /javascript/);
  assert.equal(Number(response.headers.get('content-length')), 279706);
  assert.match(await response.text(), /d3js.org v7.9.0/);
  assert.equal((await fetch(`${base}/vendor/unknown.js`)).status, 404);
});
