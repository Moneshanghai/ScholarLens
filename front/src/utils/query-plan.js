function escapeHtml(value) {
  if (value === null || value === undefined) return '';
  return String(value)
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#039;');
}

function listToTags(items) {
  if (!Array.isArray(items) || items.length === 0) {
    return '<span class="query-plan-muted">无</span>';
  }
  return items.map((item) => `<span class="query-tag">${escapeHtml(item)}</span>`).join('');
}

function boolText(value) {
  return value ? '是' : '否';
}

export function renderQueryPlanPanel(container, queryPlan) {
  if (!container) return;
  if (!queryPlan || typeof queryPlan !== 'object') {
    container.classList.add('hidden');
    container.innerHTML = '';
    return;
  }

  const llmStatus = queryPlan.llm_enabled ? '已开启' : '已关闭';
  const translated = queryPlan.used_llm_translation
    ? queryPlan.translated_keyword
    : '未翻译或与原词相同';

  container.innerHTML = `
    <div class="query-plan-header">
      <div>
        <h3><i class="bi bi-search-heart"></i> 本次实际检索词</h3>
        <p>这里展示大模型是否参与，以及最终传给检索源的关键词。</p>
      </div>
      <span class="query-plan-status ${queryPlan.llm_enabled ? 'is-on' : 'is-off'}">大模型增强：${llmStatus}</span>
    </div>
    <div class="query-plan-grid">
      <div>
        <span class="query-plan-label">原始关键词</span>
        <code>${escapeHtml(queryPlan.original_keyword)}</code>
      </div>
      <div>
        <span class="query-plan-label">主检索词</span>
        <code>${escapeHtml(queryPlan.primary_keyword)}</code>
      </div>
      <div>
        <span class="query-plan-label">翻译结果</span>
        <code>${escapeHtml(translated)}</code>
      </div>
      <div>
        <span class="query-plan-label">其他来源查询</span>
        <code>${escapeHtml(queryPlan.source_query)}</code>
      </div>
      <div class="query-plan-wide">
        <span class="query-plan-label">arXiv 查询</span>
        <code>${escapeHtml(queryPlan.arxiv_query || queryPlan.source_query)}</code>
      </div>
      <div class="query-plan-wide">
        <span class="query-plan-label">OpenAlex 查询</span>
        <code>${escapeHtml(queryPlan.openalex_query)}</code>
      </div>
      <div class="query-plan-wide">
        <span class="query-plan-label">扩展词</span>
        <div class="query-tags">${listToTags(queryPlan.expanded_terms)}</div>
      </div>
      <div class="query-plan-wide">
        <span class="query-plan-label">中文/原词回退查询</span>
        <div class="query-tags">${listToTags(queryPlan.fallback_keywords)}</div>
      </div>
    </div>
    <div class="query-plan-steps">
      <span>LLM 翻译：${boolText(queryPlan.used_llm_translation)}</span>
      <span>LLM 扩展：${boolText(queryPlan.used_llm_expansion)}</span>
      <span>LLM 筛选：${boolText(queryPlan.used_llm_filtering)}</span>
      <span>LLM 评分：${boolText(queryPlan.used_llm_scoring)}</span>
    </div>
  `;
  container.classList.remove('hidden');
}
