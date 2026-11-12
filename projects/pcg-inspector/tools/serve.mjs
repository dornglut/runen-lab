import { createServer } from 'node:http';
import { readFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { join, dirname } from 'node:path';

const root = dirname(dirname(fileURLToPath(import.meta.url)));
const assets = Object.freeze({
  '/': ['index.html', 'text/html; charset=utf-8'],
  '/index.html': ['index.html', 'text/html; charset=utf-8'],
  '/styles.css': ['styles.css', 'text/css; charset=utf-8'],
  '/vendor/d3.v7.9.0.min.js': ['vendor/d3.v7.9.0.min.js', 'text/javascript; charset=utf-8'],
  '/src/main.mjs': ['src/main.mjs', 'text/javascript; charset=utf-8'],
  '/src/sample.mjs': ['src/sample.mjs', 'text/javascript; charset=utf-8'],
  '/src/graph-data.mjs': ['src/graph-data.mjs', 'text/javascript; charset=utf-8'],
  '/fixtures/sample.json': ['fixtures/sample.json', 'application/json; charset=utf-8'],
});

export function createInspectorServer() {
  return createServer(async (request, response) => {
    response.setHeader('Cache-Control', 'no-store');
    response.setHeader('X-Content-Type-Options', 'nosniff');
    if (request.method !== 'GET' && request.method !== 'HEAD') {
      response.writeHead(405, { Allow: 'GET, HEAD' });
      response.end('Method not allowed');
      return;
    }
    let path;
    try { path = new URL(request.url, 'http://localhost').pathname; }
    catch { response.writeHead(400); response.end('Invalid path'); return; }
    const asset = Object.hasOwn(assets, path) ? assets[path] : null;
    if (!asset) { response.writeHead(404); response.end('Not found'); return; }
    try {
      const contents = await readFile(join(root, asset[0]));
      response.writeHead(200, { 'Content-Type': asset[1], 'Content-Length': contents.length });
      response.end(request.method === 'HEAD' ? undefined : contents);
    } catch {
      response.writeHead(500);
      response.end('Application asset unavailable');
    }
  });
}

export function startServer({ port = 4173, log = console.log } = {}) {
  const server = createInspectorServer();
  server.on('error', (error) => {
    if (error.code === 'EADDRINUSE') console.error(`Port ${port} already in use. Set PCG_INSPECTOR_PORT to an unused port.`);
    else console.error(`Failed to launch PCG inspector: ${error.message}`);
    process.exitCode = 1;
  });
  server.listen(port, '127.0.0.1', () => {
    log(`RunenPCG inspector: http://127.0.0.1:${server.address().port}`);
    log('Ctrl+C to stop. D3 is served from the repository-local pinned distribution.');
  });
  return server;
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  const raw = process.env.PCG_INSPECTOR_PORT ?? '4173';
  const port = Number(raw);
  if (!/^\d+$/.test(raw) || !Number.isSafeInteger(port) || port < 1 || port > 65535) {
    console.error('PCG_INSPECTOR_PORT must be a valid TCP port (1–65535).');
    process.exitCode = 2;
  } else {
    startServer({ port });
  }
}
