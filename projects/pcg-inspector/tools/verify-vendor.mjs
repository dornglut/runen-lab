import { createHash } from 'node:crypto';
import { readFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const root = dirname(dirname(fileURLToPath(import.meta.url)));
const file = await readFile(join(root, 'vendor/d3.v7.9.0.min.js'));
const expected = 'f2094bbf6141b359722c4fe454eb6c4b0f0e42cc10cc7af921fc158fceb86539';
const actual = createHash('sha256').update(file).digest('hex');
if (actual !== expected) throw new Error(`D3 v7.9.0 integrity mismatch: ${actual}`);
const license = await readFile(join(root, 'vendor/LICENSE.d3'), 'utf8');
if (!license.includes('Copyright 2010-2023 Mike Bostock') || !license.includes('Permission to use, copy, modify')) {
  throw new Error('The upstream D3 ISC license notice is missing');
}
const html = await readFile(join(root, 'index.html'), 'utf8');
if (!html.includes('src="./vendor/d3.v7.9.0.min.js"') || /<script[^>]+src="https?:/i.test(html)) {
  throw new Error('The inspector must load its verified local D3 script without a remote script dependency');
}
console.log(`Verified offline D3 v7.9.0: ${actual}`);
