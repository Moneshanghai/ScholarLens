import { hasPdfFile } from './pdf-sort.js';

export function sortPapers(papers, column, direction = 'asc') {
  papers.sort((a, b) => comparePapers(a, b, column, direction));
  return papers;
}

export function defaultSortDirection(column) {
  return ['if_score', 'year', 'has_pdf', 'relevance_score'].includes(column) ? 'desc' : 'asc';
}

export function comparePapers(a, b, column, direction = 'asc') {
  const [valA, valB] = valuesForColumn(a, b, column);

  if (valA < valB) return direction === 'asc' ? -1 : 1;
  if (valA > valB) return direction === 'asc' ? 1 : -1;
  return 0;
}

function valuesForColumn(a, b, column) {
  switch (column) {
    case 'title':
      return [stringValue(a.title), stringValue(b.title)];
    case 'authors':
      return [getAuthorsString(a.authors).toLowerCase(), getAuthorsString(b.authors).toLowerCase()];
    case 'journal':
      return [getJournalName(a).toLowerCase(), getJournalName(b).toLowerCase()];
    case 'if_score':
      return [getImpactFactorValue(a), getImpactFactorValue(b)];
    case 'year':
      return [parseInt(a.year, 10) || 0, parseInt(b.year, 10) || 0];
    case 'relevance_score':
      return [getRelevanceScoreValue(a), getRelevanceScoreValue(b)];
    case 'has_pdf':
      return [hasPdfFile(a) ? 1 : 0, hasPdfFile(b) ? 1 : 0];
    default:
      return [0, 0];
  }
}

export function getAuthorsString(authors) {
  if (!authors) return '';
  if (typeof authors === 'string') return authors;
  if (Array.isArray(authors)) return authors.join(', ');
  return String(authors);
}

export function getJournalName(paper) {
  return paper.journal || paper.venue || paper.publicationVenue || paper.containerTitle || '';
}

export function getImpactFactorValue(paper) {
  const ifValue = paper.if_score || paper.sciif || paper.impactFactor || paper.if || paper.IF;
  return parseFloat(ifValue) || 0;
}

export function getRelevanceScoreValue(paper) {
  const value = paper.relevance_score ?? paper.relevanceScore;
  const score = Number(value);
  return Number.isFinite(score) ? score : 0;
}

function stringValue(value) {
  return (value || '').toLowerCase();
}
