import { validateGraphDocument, parseGraphJson, layoutRandomSource, GraphDataError } from './graph-data.mjs';
import { sample } from './sample.mjs';

const $ = (id) => document.getElementById(id);
const d3 = globalThis.d3;
const graphSvg = $('graph-svg');
const banner = $('error-banner');

function showError(message) {
  banner.textContent = message;
  banner.hidden = false;
  $('status').textContent = 'Input rejected';
  clearTimeout(showError.timeout);
  showError.timeout = setTimeout(() => { banner.hidden = true; }, 9000);
}

if (!d3) {
  showError('D3 failed to load. The first prototype requires internet access to its pinned D3 CDN file.');
  $('status').textContent = 'D3 unavailable';
} else {
  initialize();
}

function initialize() {
  const svg = d3.select(graphSvg);
  const viewport = svg.append('g').attr('class', 'graph-viewport');
  const edgeLayer = viewport.append('g').attr('class', 'edges');
  const haloLayer = viewport.append('g').attr('class', 'halos');
  const nodeLayer = viewport.append('g').attr('class', 'nodes');
  const labelLayer = viewport.append('g').attr('class', 'labels');
  const zoom = d3.zoom().scaleExtent([0.15, 4]).on('zoom', (event) => viewport.attr('transform', event.transform));
  svg.call(zoom).on('dblclick.zoom', null);
  let simulation = null;
  let renderedNodes = [];
  let renderedEdges = [];
  let edgeShapes = null;
  let haloShapes = null;
  let nodeShapes = null;
  let labelShapes = null;
  let graph = null;
  let selectedNode = null;
  let paused = false;
  const reduced = matchMedia('(prefers-reduced-motion: reduce)').matches;
  const settings = { charge: -220, linkDistance: 96, collision: 22, damping: 0.40 };

  function labelStatus(value) { $('status').textContent = value; }

  function updateSimulation() {
    if (!simulation) return;
    simulation.force('charge').strength(settings.charge);
    simulation.force('link').distance(settings.linkDistance);
    simulation.force('collision').radius(settings.collision);
    simulation.velocityDecay(settings.damping);
    if (!paused && !reduced) simulation.alpha(0.65).restart();
    else { simulation.stop(); simulation.alpha(0.4); simulation.tick(60); tick(); }
  }

  function tick() {
    if (!edgeShapes || !nodeShapes) return;
    edgeShapes.attr('x1', d => d.source.x).attr('y1', d => d.source.y)
      .attr('x2', d => d.target.x).attr('y2', d => d.target.y);
    haloShapes.attr('cx', d => d.x).attr('cy', d => d.y);
    nodeShapes.attr('cx', d => d.x).attr('cy', d => d.y);
    labelShapes.attr('x', d => d.x).attr('y', d => d.y);
  }

  function styleSelection() {
    const hasSelection = selectedNode !== null;
    const neighborhood = new Set(hasSelection ? [selectedNode] : []);
    if (hasSelection) {
      for (const edge of graph.edges) {
        if (edge[0] === selectedNode) neighborhood.add(edge[1]);
        if (edge[1] === selectedNode) neighborhood.add(edge[0]);
      }
    }
    nodeShapes.classed('selected', d => d.id === selectedNode)
      .classed('faded', d => hasSelection && !neighborhood.has(d.id));
    edgeShapes.classed('highlight', d => hasSelection && (d.source.id === selectedNode || d.target.id === selectedNode))
      .classed('faded', d => hasSelection && d.source.id !== selectedNode && d.target.id !== selectedNode);
    labelShapes.classed('hidden', !$('show-labels').checked);
    if (!hasSelection) {
      $('selection-title').textContent = 'No node selected';
      $('selection-help').textContent = 'Select a node to inspect its degree and neighbors.';
      $('selection-body').replaceChildren();
      return;
    }
    $('selection-title').textContent = `Node ${selectedNode}`;
    $('selection-help').textContent = 'Connected neighbors and structural properties';
    const details = $('selection-body');
    details.replaceChildren();
    const degree = document.createElement('p');
    degree.textContent = `Degree: ${graph.degree[selectedNode]}`;
    details.append(degree);
    const heading = document.createElement('p');
    heading.textContent = 'Neighbors';
    details.append(heading);
    const pills = document.createElement('div');
    pills.className = 'node-pills';
    const neighbors = [...neighborhood].filter(id => id !== selectedNode).sort((a, b) => a - b);
    for (const id of neighbors) {
      const pill = document.createElement('span');
      pill.className = 'node-pill';
      pill.textContent = String(id);
      pills.append(pill);
    }
    if (!neighbors.length) pills.textContent = 'None';
    details.append(pills);
    const pinned = renderedNodes.find(d => d.id === selectedNode)?.pinned;
    const pinInfo = document.createElement('p');
    pinInfo.textContent = pinned ? 'Pinned in this view.' : 'Not pinned.';
    pinInfo.style.marginTop = '9px';
    details.append(pinInfo);
  }

  function selectNode(id) {
    selectedNode = selectedNode === id ? null : id;
    styleSelection();
  }

  function pinToggle(node) {
    node.pinned = !node.pinned;
    node.fx = node.pinned ? node.x : null;
    node.fy = node.pinned ? node.y : null;
    haloShapes.classed('pinned', d => d.pinned);
    styleSelection();
    labelStatus(node.pinned ? `Pinned node ${node.id}` : `Released node ${node.id}`);
  }

  function createView(document) {
    graph = validateGraphDocument(document);
    if (simulation) simulation.stop();
    paused = reduced;
    selectedNode = null;
    renderedNodes = Array.from({ length: graph.nodeCount }, (_, id) => ({ id, pinned: false }));
    renderedEdges = graph.edges.map(([source, target]) => ({ source, target }));
    const rand = layoutRandomSource(graph.seed);
    // Explicitly initialize positions, so the layout is repeatable for its own settings.
    for (const node of renderedNodes) {
      const angle = rand() * Math.PI * 2;
      const radius = Math.sqrt(rand()) * Math.max(30, Math.sqrt(graph.nodeCount) * 23);
      node.x = Math.cos(angle) * radius;
      node.y = Math.sin(angle) * radius;
      node.vx = node.vy = 0;
    }

    edgeShapes = edgeLayer.selectAll('line').data(renderedEdges).join('line').attr('class', 'graph-edge');
    haloShapes = haloLayer.selectAll('circle').data(renderedNodes, d => d.id)
      .join('circle').attr('class', 'node-halo').attr('r', 17);
    nodeShapes = nodeLayer.selectAll('circle').data(renderedNodes, d => d.id)
      .join('circle').attr('class', d => graph.degree[d.id] >= 3 ? 'graph-node hub' : 'graph-node')
      .attr('r', 9).attr('tabindex', 0).attr('role', 'button')
      .attr('aria-label', d => `Node ${d.id}, degree ${graph.degree[d.id]}`)
      .on('click', (event, d) => { event.stopPropagation(); selectNode(d.id); })
      .on('dblclick', (event, d) => { event.stopPropagation(); pinToggle(d); })
      .on('keydown', (event, d) => {
        if (event.key === 'p' || event.key === 'P') { event.preventDefault(); pinToggle(d); }
        if (event.key === 'Enter' || event.key === ' ') { event.preventDefault(); selectNode(d.id); }
      });
    labelShapes = labelLayer.selectAll('text').data(renderedNodes, d => d.id)
      .join('text').attr('class', 'node-label').text(d => d.id);
    const drag = d3.drag()
      .on('start', (event, d) => {
        event.sourceEvent.stopPropagation();
        if (!paused && !reduced && !event.active) simulation.alphaTarget(0.28).restart();
        d.fx = d.x; d.fy = d.y;
      })
      .on('drag', (event, d) => { d.fx = event.x; d.fy = event.y; d.x = event.x; d.y = event.y; tick(); })
      .on('end', (event, d) => {
        if (!paused && !reduced && !event.active) simulation.alphaTarget(0);
        if (!d.pinned) { d.fx = null; d.fy = null; }
        tick();
      });
    nodeShapes.call(drag);

    simulation = d3.forceSimulation(renderedNodes).stop().randomSource(rand)
      .force('link', d3.forceLink(renderedEdges).id(d => d.id).distance(settings.linkDistance).strength(0.7))
      .force('charge', d3.forceManyBody().strength(settings.charge).distanceMax(700))
      .force('collision', d3.forceCollide(settings.collision).iterations(2))
      .force('center', d3.forceCenter(0, 0))
      .velocityDecay(settings.damping).alphaDecay(0.032)
      .on('tick', tick)
      .on('end', () => { labelStatus('Layout settled · drag to interact'); });

    // Initial settling before the first visible frame reduces severe overlaps.
    simulation.tick(Math.min(110, Math.max(35, graph.nodeCount)));
    tick();
    if (!reduced) simulation.alpha(0.6).restart();
    else { simulation.tick(160); tick(); }
    $('toggle-button').textContent = paused ? 'Resume motion' : 'Pause motion';
    $('node-count').textContent = graph.nodeCount.toLocaleString();
    $('edge-count').textContent = graph.edges.length.toLocaleString();
    $('cycle-count').textContent = graph.metrics.cycleRank.toLocaleString();
    $('branch-count').textContent = graph.metrics.branchingVertices.toLocaleString();
    $('algorithm').textContent = graph.algorithm;
    $('seed').textContent = graph.seed;
    styleSelection();
    labelStatus(reduced ? 'Static layout · reduced-motion preference' : 'Simulating · drag to inspect');
    requestAnimationFrame(() => fitView());
  }

  function fitView() {
    if (!renderedNodes.length) return;
    const frame = $('drop-zone').getBoundingClientRect();
    const width = Math.max(100, frame.width), height = Math.max(100, frame.height);
    // SVG viewBox is fixed; view coordinates below are converted to that space.
    const baseWidth = 1200, baseHeight = 800;
    const padding = 65;
    const x0 = Math.min(...renderedNodes.map(d => d.x));
    const x1 = Math.max(...renderedNodes.map(d => d.x));
    const y0 = Math.min(...renderedNodes.map(d => d.y));
    const y1 = Math.max(...renderedNodes.map(d => d.y));
    const extentX = Math.max(65, x1 - x0 + 2 * padding);
    const extentY = Math.max(65, y1 - y0 + 2 * padding);
    const viewportAspect = width / height;
    const svgAspect = baseWidth / baseHeight;
    const aspectScale = viewportAspect > svgAspect ? svgAspect / viewportAspect : 1;
    const scale = Math.min(1.65, 0.85 * Math.min(baseWidth / extentX, baseHeight / extentY) * aspectScale);
    const centerX = (x0 + x1) / 2, centerY = (y0 + y1) / 2;
    svg.transition().duration(reduced ? 0 : 400).call(
      zoom.transform, d3.zoomIdentity.translate(baseWidth / 2 - scale * centerX, baseHeight / 2 - scale * centerY).scale(scale)
    );
  }

  function exportSvg() {
    if (!graph) return;
    const copy = graphSvg.cloneNode(true);
    copy.setAttribute('xmlns', 'http://www.w3.org/2000/svg');
    copy.setAttribute('width', '1200'); copy.setAttribute('height', '800');
    const defs = document.createElementNS('http://www.w3.org/2000/svg', 'style');
    defs.textContent = '.graph-edge{stroke:#506382;stroke-width:1.6;opacity:.8}.graph-edge.highlight{stroke:#b0c0ff;stroke-width:2.5;opacity:1}.graph-edge.faded{opacity:.13}.graph-node{fill:#6f8fff;stroke:#a0b6ff;stroke-width:1.4}.graph-node.hub{fill:#63c4d3;stroke:#ace5ec}.graph-node.selected{fill:#e4a979;stroke:#ffe4be;stroke-width:2.6}.graph-node.faded{opacity:.24}.node-label{fill:#edf1ff;text-anchor:middle;dominant-baseline:central;font:600 11px monospace}.node-label.hidden{display:none}.node-halo{fill:none;stroke:#7c97fd;stroke-width:1;opacity:0}.node-halo.pinned{opacity:1;stroke-dasharray:3 5}';
    copy.insertBefore(defs, copy.firstChild);
    const blob = new Blob([new XMLSerializer().serializeToString(copy)], { type: 'image/svg+xml' });
    const url = URL.createObjectURL(blob);
    const anchor = document.createElement('a');
    anchor.href = url; anchor.download = `runen-pcg-graph-${graph.seed}.svg`;
    anchor.click();
    setTimeout(() => URL.revokeObjectURL(url), 1000);
    labelStatus('Exported SVG snapshot');
  }

  const sliders = [['charge', 'charge'], ['link-distance', 'linkDistance'], ['collision', 'collision'], ['damping', 'damping']];
  for (const [id, key] of sliders) {
    const control = $(id);
    control.addEventListener('input', () => {
      settings[key] = Number(control.value);
      $(`${id}-value`).textContent = key === 'damping' ? settings[key].toFixed(2) : String(settings[key]);
      updateSimulation();
    });
  }
  $('show-labels').addEventListener('change', styleSelection);
  $('fit-button').addEventListener('click', fitView);
  $('reset-button').addEventListener('click', () => { if (graph) createView({ format: graph.format, algorithm: graph.algorithm, seed: graph.seed, node_count: graph.nodeCount, edges: graph.edges }); });
  $('toggle-button').addEventListener('click', () => {
    paused = !paused;
    if (paused) { simulation.stop(); labelStatus('Simulation paused'); }
    else { simulation.alphaTarget(0).alpha(0.5).restart(); labelStatus('Simulating'); }
    $('toggle-button').textContent = paused ? 'Resume motion' : 'Pause motion';
  });
  $('export-button').addEventListener('click', exportSvg);
  $('sample-button').addEventListener('click', () => createView(sample));
  svg.on('click', (event) => { if (event.target === graphSvg) { selectedNode = null; styleSelection(); } });

  async function importFile(file) {
    if (!file) return;
    if (file.size > 512_000) { showError('File is larger than 512 KB.'); return; }
    try { createView(parseGraphJson(await file.text())); }
    catch (error) { showError(error instanceof GraphDataError ? error.message : 'Failed to read graph file.'); }
  }
  $('file-input').addEventListener('change', (event) => {
    void importFile(event.target.files?.[0]);
    event.target.value = '';
  });
  const drop = $('drop-zone');
  drop.addEventListener('dragover', event => { event.preventDefault(); drop.classList.add('is-dragging'); });
  drop.addEventListener('dragleave', () => drop.classList.remove('is-dragging'));
  drop.addEventListener('drop', event => {
    event.preventDefault(); drop.classList.remove('is-dragging');
    void importFile(event.dataTransfer?.files?.[0]);
  });
  createView(sample);
}
