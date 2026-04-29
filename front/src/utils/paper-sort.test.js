import assert from 'node:assert/strict';
import test from 'node:test';

import { getRelevanceScoreValue, sortPapers } from './paper-sort.js';

test('sortPapers reorders by relevance after impact factor sorting', () => {
  const papers = [
    { title: 'best semantic match', relevance_score: 95, if_score: '1.0' },
    { title: 'high impact weak match', relevance_score: 40, if_score: '30.0' },
    { title: 'middle match', relevance_score: 80, if_score: '8.0' },
  ];

  sortPapers(papers, 'if_score', 'desc');
  assert.deepEqual(papers.map((paper) => paper.title), [
    'high impact weak match',
    'middle match',
    'best semantic match',
  ]);

  sortPapers(papers, 'relevance_score', 'desc');
  assert.deepEqual(papers.map((paper) => paper.title), [
    'best semantic match',
    'middle match',
    'high impact weak match',
  ]);
});

test('getRelevanceScoreValue accepts snake_case and camelCase scores', () => {
  assert.equal(getRelevanceScoreValue({ relevance_score: 87 }), 87);
  assert.equal(getRelevanceScoreValue({ relevanceScore: '76' }), 76);
  assert.equal(getRelevanceScoreValue({ relevance_score: 'bad' }), 0);
});
