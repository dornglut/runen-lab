// Pin to RunenPCG's public v1 integration-test golden vector: 8 nodes,
// 4 additional edges, seed 42. This is a view fixture, not generated in JS.
export const sample = Object.freeze({
  format: 'runen-lab.graph-view.v1',
  algorithm: 'runen-pcg.graph.spanning-tree.v1',
  seed: '42',
  node_count: 8,
  edges: [
    [0, 1], [1, 3], [1, 5], [1, 6], [1, 7], [2, 4],
    [2, 6], [3, 4], [4, 5], [4, 7], [6, 7],
  ],
});
