import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { GraphDataError, MAX_VIEW_NODES, FORMAT, validateGraphDocument, parseGraphJson, layoutRandomSource } from '../src/graph-data.mjs';
import { sample } from '../src/sample.mjs';

const fixture = JSON.parse(readFileSync(new URL('../fixtures/sample.json', import.meta.url), 'utf8'));
const changes = (data) => structuredClone(data);
const fails = (value, pattern) => assert.throws(() => validateGraphDocument(value), { name: 'GraphDataError', message: pattern });

test('fixture matches accepted RunenPCG v1 test vector and derives metrics', () => {
  assert.deepEqual(fixture, sample);
  const graph = validateGraphDocument(fixture);
  assert.equal(graph.nodeCount, 8);
  assert.equal(graph.edges.length, 11);
  assert.deepEqual(graph.metrics, { cycleRank: 4, branchingVertices: 4 });
  assert.equal(graph.degree.reduce((a, b) => a + b, 0), 22);
});

test('single-node graph is valid', () => {
  const graph = validateGraphDocument({ format: FORMAT, algorithm: 'test.v1', seed: '0', node_count: 1, edges: [] });
  assert.equal(graph.metrics.cycleRank, 0);
  assert.equal(graph.metrics.branchingVertices, 0);
});

test('rejects unknown schema, missing fields, and bad types', () => {
  for (const value of [null, {}, [], 2, 'string', { ...sample, format: 'other' }]) {
    assert.throws(() => validateGraphDocument(value), GraphDataError);
  }
  fails({ ...sample, node_count: 0 }, /node_count/);
  fails({ ...sample, node_count: MAX_VIEW_NODES + 1 }, /node_count/);
  fails({ ...sample, node_count: 5.5 }, /node_count/);
  fails({ ...sample, algorithm: '<img>' }, /algorithm/);
});

test('protects full u64 seeds from JSON-number precision loss', () => {
  for (const badSeed of [42, '-1', '01', '18446744073709551616', 'none', '', '1e5']) {
    fails({ ...sample, seed: badSeed }, /seed/);
  }
  const graph = validateGraphDocument({ ...sample, seed: '18446744073709551615' });
  assert.equal(graph.seed, '18446744073709551615');
});

test('rejects loops, reversed, missing, and noninteger endpoints', () => {
  for (const pair of [[2, 2], [4, 1], [-1, 2], [0, 8], [0, '2'], [0], [0, 1, 2], [NaN, 3]]) {
    fails({ ...sample, edges: [pair] }, /edges\[0\]/);
  }
});

test('rejects duplicate and unsorted edges', () => {
  const repeated = changes(sample); repeated.edges.splice(1, 0, [0, 1]);
  fails(repeated, /unique/);
  const unsorted = changes(sample); [unsorted.edges[0], unsorted.edges[1]] = [unsorted.edges[1], unsorted.edges[0]];
  fails(unsorted, /sorted/);
});

test('rejects disconnected graph despite enough edges', () => {
  const disconnected = { ...sample, node_count: 5, edges: [[0,1], [0,2], [1,2], [3,4]] };
  fails(disconnected, /disconnected/);
});

test('rejects too few edges and too many edges for visual capacity', () => {
  fails({ ...sample, edges: [[0, 1]] }, /few edges/);
  fails({ ...sample, edges: Array(2501).fill([0,1]) }, /at most/);
});

test('does not accept caller claims about structural quality', () => {
  const graph = validateGraphDocument({ ...sample, metrics: { cycleRank: 999, branchingVertices: 999 } });
  assert.deepEqual(graph.metrics, { cycleRank: 4, branchingVertices: 4 });
});

test('JSON parsing rejects malformed or oversized input', () => {
  assert.equal(parseGraphJson(JSON.stringify(sample)).nodeCount, 8);
  assert.throws(() => parseGraphJson('{'), /Invalid JSON/);
  assert.throws(() => parseGraphJson(' '.repeat(512_001)), /512 KB/);
});

test('freezes accepted result and canonical pairs', () => {
  const graph = validateGraphDocument(sample);
  assert.ok(Object.isFrozen(graph));
  assert.ok(Object.isFrozen(graph.edges));
  assert.ok(Object.isFrozen(graph.edges[0]));
});

test('layout random source repeats and changes with independent seed', () => {
  const first = layoutRandomSource('42');
  const second = layoutRandomSource('42');
  const other = layoutRandomSource('43');
  assert.deepEqual([first(), first(), first()], [second(), second(), second()]);
  assert.notEqual(layoutRandomSource('42')(), other());
});
