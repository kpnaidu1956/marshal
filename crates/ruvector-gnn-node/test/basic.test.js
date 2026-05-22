// Basic tests for Ruvector GNN Node.js bindings

const { test } = require('node:test');
const assert = require('node:assert');

const {
  RuvectorLayer,
  TensorCompress,
  differentiableSearch,
  hierarchicalForward,
  getCompressionLevel,
  init
} = require('../index.js');

test('initialization', () => {
  const result = init();
  assert.strictEqual(typeof result, 'string');
  assert.ok(result.includes('initialized'));
});

test('RuvectorLayer creation', () => {
  const layer = new RuvectorLayer(4, 8, 2, 0.1);
  assert.ok(layer instanceof RuvectorLayer);
});

test('RuvectorLayer forward pass', () => {
  const layer = new RuvectorLayer(4, 8, 2, 0.1);
  const node = [1.0, 2.0, 3.0, 4.0];
  const neighbors = [[0.5, 1.0, 1.5, 2.0], [2.0, 3.0, 4.0, 5.0]];
  const weights = [0.3, 0.7];

  const output = layer.forward(node, neighbors, weights);
  assert.strictEqual(output.length, 8);
  assert.ok(output.every(x => typeof x === 'number'));
});

test('RuvectorLayer forward with no neighbors', () => {
  const layer = new RuvectorLayer(4, 8, 2, 0.1);
  const node = [1.0, 2.0, 3.0, 4.0];
  const neighbors = [];
  const weights = [];

  const output = layer.forward(node, neighbors, weights);
  assert.strictEqual(output.length, 8);
});

test('RuvectorLayer serialization', () => {
  const layer = new RuvectorLayer(4, 8, 2, 0.1);
  const json = layer.toJson();
  assert.strictEqual(typeof json, 'string');
  assert.ok(json.length > 0);
});

test('RuvectorLayer deserialization', () => {
  const layer1 = new RuvectorLayer(4, 8, 2, 0.1);
  const json = layer1.toJson();
  const layer2 = RuvectorLayer.fromJson(json);

  assert.ok(layer2 instanceof RuvectorLayer);

  // Test that they produce same output
  const node = [1.0, 2.0, 3.0, 4.0];
  const neighbors = [[0.5, 1.0, 1.5, 2.0]];
  const weights = [1.0];

  const output1 = layer1.forward(node, neighbors, weights);
  const output2 = layer2.forward(node, neighbors, weights);

  assert.strictEqual(output1.length, output2.length);
  output1.forEach((val, i) => {
    assert.ok(Math.abs(val - output2[i]) < 1e-6);
  });
});

test('TensorCompress creation', () => {
  const compressor = new TensorCompress();
  assert.ok(compressor instanceof TensorCompress);
});

test('TensorCompress adaptive compression', () => {
  const compressor = new TensorCompress();
  const embedding = [1.0, 2.0, 3.0, 4.0];

  const compressed = compressor.compress(embedding, 0.5);
  assert.strictEqual(typeof compressed, 'string');
  assert.ok(compressed.length > 0);
});

test('TensorCompress round-trip', () => {
  const compressor = new TensorCompress();
  const embedding = [1.0, 2.0, 3.0, 4.0];

  const compressed = compressor.compress(embedding, 1.0); // No compression
  const decompressed = compressor.decompress(compressed);

  assert.strictEqual(decompressed.length, embedding.length);
  decompressed.forEach((val, i) => {
    assert.ok(Math.abs(val - embedding[i]) < 1e-6);
  });
});

test('TensorCompress with explicit level', () => {
  const compressor = new TensorCompress();
  const embedding = Array.from({ length: 64 }, (_, i) => i * 0.1);

  const level = {
    level_type: 'half',
    scale: 1.0
  };

  const compressed = compressor.compressWithLevel(embedding, level);
  const decompressed = compressor.decompress(compressed);

  assert.strictEqual(decompressed.length, embedding.length);
});

test('getCompressionLevel', () => {
  assert.strictEqual(getCompressionLevel(0.9), 'none');
  assert.strictEqual(getCompressionLevel(0.5), 'half');
  assert.strictEqual(getCompressionLevel(0.2), 'pq8');
  assert.strictEqual(getCompressionLevel(0.05), 'pq4');
  assert.strictEqual(getCompressionLevel(0.001), 'binary');
});

test('differentiableSearch', () => {
  const query = [1.0, 0.0, 0.0];
  const candidates = [
    [1.0, 0.0, 0.0],
    [0.9, 0.1, 0.0],
    [0.0, 1.0, 0.0],
  ];

  const result = differentiableSearch(query, candidates, 2, 1.0);

  assert.ok(Array.isArray(result.indices));
  assert.ok(Array.isArray(result.weights));
  assert.strictEqual(result.indices.length, 2);
  assert.strictEqual(result.weights.length, 2);

  // First result should be perfect match
  assert.strictEqual(result.indices[0], 0);

  // Weights should be valid probabilities
  result.weights.forEach(w => {
    assert.ok(w >= 0 && w <= 1);
  });
});

test('differentiableSearch with empty candidates', () => {
  const query = [1.0, 0.0, 0.0];
  const candidates = [];

  const result = differentiableSearch(query, candidates, 2, 1.0);

  assert.strictEqual(result.indices.length, 0);
  assert.strictEqual(result.weights.length, 0);
});

test('hierarchicalForward', () => {
  const query = [1.0, 0.0];
  const layerEmbeddings = [
    [[1.0, 0.0], [0.0, 1.0]],
  ];

  const layer = new RuvectorLayer(2, 2, 1, 0.0);
  const layers = [layer.toJson()];

  const result = hierarchicalForward(query, layerEmbeddings, layers);

  assert.ok(Array.isArray(result));
  assert.strictEqual(result.length, 2);
  assert.ok(result.every(x => typeof x === 'number'));
});

test('invalid dropout rate throws error', () => {
  assert.throws(() => {
    new RuvectorLayer(4, 8, 2, 1.5); // dropout > 1.0
  });

  assert.throws(() => {
    new RuvectorLayer(4, 8, 2, -0.1); // dropout < 0.0
  });
});

test('compression with empty embedding throws error', () => {
  const compressor = new TensorCompress();
  assert.throws(() => {
    compressor.compress([], 0.5);
  });
});

test('compression levels produce different sizes', () => {
  const compressor = new TensorCompress();
  const embedding = Array.from({ length: 64 }, (_, i) => Math.sin(i * 0.1));

  const none = compressor.compress(embedding, 1.0);    // No compression
  const half = compressor.compress(embedding, 0.5);    // Half precision
  const binary = compressor.compress(embedding, 0.001); // Binary

  // Binary should be smallest
  assert.ok(binary.length < half.length);
  // None should be largest (or close to half)
  assert.ok(none.length >= half.length * 0.8);
});
