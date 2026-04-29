import assert from 'node:assert/strict';
import test from 'node:test';

import { getPdfUrl, hasPdfFile } from './pdf-sort.js';

test('hasPdfFile treats non-empty pdf_url as available', () => {
  assert.equal(hasPdfFile({ pdf_url: 'https://example.com/paper.pdf' }), true);
});

test('hasPdfFile treats empty or whitespace pdf_url as unavailable', () => {
  assert.equal(hasPdfFile({ pdf_url: '' }), false);
  assert.equal(hasPdfFile({ pdf_url: '   ' }), false);
  assert.equal(hasPdfFile({}), false);
});

test('getPdfUrl trims supported PDF URL fields', () => {
  assert.equal(getPdfUrl({ pdf_url: ' https://example.com/a.pdf ' }), 'https://example.com/a.pdf');
  assert.equal(getPdfUrl({ pdfUrl: 'https://example.com/b.pdf' }), 'https://example.com/b.pdf');
  assert.equal(getPdfUrl({ pdf: 'https://example.com/c.pdf' }), 'https://example.com/c.pdf');
});
