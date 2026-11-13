/**
 * Consumer-owned interchange contract for a RunenPCG visualization.
 * This is not RunenPCG serialization or a framework API.
 */
export const FORMAT = 'runen-lab.graph-view.v1';
export const MAX_VIEW_NODES = 500;
export const MAX_VIEW_EDGES = 2500;
const U64_MAX = 18_446_744_073_709_551_615n;

export class GraphDataError extends Error {
  constructor(message) {
    super(message);
    this.name = 'GraphDataError';
  }
}

const reject = (message) => { throw new GraphDataError(message); };
const object = (value) => value !== null && typeof value === 'object' && !Array.isArray(value);
const integer = (value) => Number.isSafeInteger(value);

export function validateGraphDocument(document) {
  if (!object(document)) reject('Expected a JSON object.');
  if (document.format !== FORMAT) reject(`Unsupported format: expected ${FORMAT}.`);
  const { node_count: count, edges, algorithm, seed } = document;
  if (!integer(count) || count < 1 || count > MAX_VIEW_NODES) {
    reject(`node_count must be an integer between 1 and ${MAX_VIEW_NODES}.`);
  }
  if (!Array.isArray(edges) || edges.length > MAX_VIEW_EDGES) {
    reject(`edges must be an array with at most ${MAX_VIEW_EDGES} pairs.`);
  }
  if (typeof algorithm !== 'string' || !/^[a-zA-Z0-9_.-]{1,128}$/.test(algorithm)) {
    reject('algorithm must be a short, nonempty identifier.');
  }
  if (typeof seed !== 'string' || !/^(0|[1-9][0-9]*)$/.test(seed) ||
      seed.length > 20 || BigInt(seed) > U64_MAX) {
    reject('seed must be a canonical decimal string representing a u64.');
  }

  const normalized = [];
  const degree = new Uint32Array(count);
  const adjacency = Array.from({ length: count }, () => []);
  let previous = null;
  for (let i = 0; i < edges.length; i++) {
    const pair = edges[i];
    if (!Array.isArray(pair) || pair.length !== 2 || !integer(pair[0]) || !integer(pair[1])) {
      reject(`edges[${i}] must contain exactly two integer endpoints.`);
    }
    const [a, b] = pair;
    if (a < 0 || a >= b || b >= count) {
      reject(`edges[${i}] must satisfy 0 <= first < second < node_count.`);
    }
    if (previous && (a < previous[0] || (a === previous[0] && b <= previous[1]))) {
      reject(`edges[${i}] must be unique and lexicographically sorted.`);
    }
    previous = [a, b];
    normalized.push(Object.freeze([a, b]));
    degree[a]++;
    degree[b]++;
    adjacency[a].push(b);
    adjacency[b].push(a);
  }

  if (edges.length < count - 1) reject('Graph has too few edges to be connected.');
  const reached = new Uint8Array(count);
  reached[0] = 1;
  const stack = [0];
  let visited = 1;
  while (stack.length) {
    const current = stack.pop();
    for (const neighbor of adjacency[current]) {
      if (!reached[neighbor]) {
        reached[neighbor] = 1;
        visited++;
        stack.push(neighbor);
      }
    }
  }
  if (visited !== count) reject('Graph is disconnected.');

  const cycleRank = edges.length - count + 1;
  const branching = degree.reduce((total, value) => total + (value >= 3 ? 1 : 0), 0);
  return Object.freeze({
    format: FORMAT,
    nodeCount: count,
    edges: Object.freeze(normalized),
    algorithm,
    seed,
    metrics: Object.freeze({ cycleRank, branchingVertices: branching }),
    degree: Object.freeze(Array.from(degree)),
  });
}

/** Strictly parses user-controlled JSON without executing or trusting metadata. */
export function parseGraphJson(text) {
  if (typeof text !== 'string' || text.length > 512_000) {
    reject('Graph JSON must be text smaller than 512 KB.');
  }
  let payload;
  try { payload = JSON.parse(text); }
  catch { reject('Invalid JSON.'); }
  return validateGraphDocument(payload);
}

/** Deterministic, local presentation RNG. NOT RunenPCG generation/replay. */
export function layoutRandomSource(seed) {
  let state = 0x811c9dc5;
  for (const ch of seed) {
    state ^= ch.charCodeAt(0);
    state = Math.imul(state, 0x01000193);
  }
  return () => {
    state ^= state << 13;
    state ^= state >>> 17;
    state ^= state << 5;
    return (state >>> 0) / 0x1_0000_0000;
  };
}
