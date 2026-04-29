export function hasPdfFile(paper) {
  return getPdfUrl(paper).length > 0;
}

export function getPdfUrl(paper) {
  const pdfUrl = paper?.pdf_url ?? paper?.pdfUrl ?? paper?.pdf ?? '';
  if (typeof pdfUrl === 'string') {
    return pdfUrl.trim();
  }
  return pdfUrl ? String(pdfUrl).trim() : '';
}
