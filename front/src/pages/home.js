/**
 * Home Page - Search Form and Results
 */

import { createTask, pollTaskStatus, downloadCSV, downloadBibTeX, fetchSources } from '../api/client.js';
import { historyManager } from '../utils/history.js';
import { renderQueryPlanPanel } from '../utils/query-plan.js';
import { getPdfUrl } from '../utils/pdf-sort.js';
import {
  defaultSortDirection,
  getAuthorsString,
  getImpactFactorValue,
  getJournalName,
  getRelevanceScoreValue,
  sortPapers as sortPaperRows,
} from '../utils/paper-sort.js';
import { router, renderHistoryList, escapeHtml } from '../main.js';

export class HomePage {
  constructor() {
    this.currentTaskId = null;
    this.currentPapers = [];
    this.currentSortMode = 'relevance';
    this.currentSortColumn = 'relevance_score';
    this.currentSortDirection = 'desc';
    this.initialSortColumn = 'relevance_score';
    this.initialSortDirection = 'desc';
  }

  render() {
    const currentYear = new Date().getFullYear();
    const defaultYear = currentYear - 5;

    return `
      <!-- Header with History Toggle -->
      <header class="header">
        <div class="container header-container">
          <a href="/" class="header-logo">ScholarLens</a>
          <nav class="header-nav">
            <button id="history-toggle" class="btn-history" aria-label="查询历史">
              <i class="bi bi-clock-history"></i>
              <span>历史记录</span>
            </button>
            <a href="/docs" class="header-link"><i class="bi bi-file-earmark-text"></i> <span>API 文档</span></a>
            <a href="/settings" class="header-link"><i class="bi bi-gear"></i> <span>模型配置</span></a>
            <a href="/logout" data-logout class="header-link header-link-logout" title="登出"><i class="bi bi-box-arrow-right"></i> <span>登出</span></a>
          </nav>
        </div>
      </header>

      <!-- Hero Section -->
      <section class="hero">
        <div class="container">
          <div class="hero-icon"><i class="bi bi-journal-richtext"></i></div>
          <div class="hero-meta">
            <h1 class="hero-title">学术文献智能搜索</h1>
            <p class="hero-subtitle">多源聚合 · 智能筛选 · 一站式学术检索体验</p>
          </div>
          <div class="hero-badges">
            <span class="badge badge-primary"><i class="bi bi-stars"></i> AI 检索</span>
            <span class="badge badge-info"><i class="bi bi-diagram-3"></i> 多源聚合</span>
            <span class="badge badge-success"><i class="bi bi-funnel"></i> 文献筛选</span>
          </div>
        </div>
      </section>

      <!-- Search Section -->
      <section class="search-section">
        <div class="container">
          <div class="search-card">
            <div class="section-heading">
              <div class="section-icon"><i class="bi bi-search"></i></div>
              <div>
                <h2 class="section-title">开始搜索</h2>
                <p class="section-description">输入关键词与筛选条件，创建后台文献检索任务。</p>
              </div>
            </div>
            
            <form id="search-form" class="search-form">
              <!-- Keyword -->
              <div class="form-group">
                <label for="keyword" class="form-label">搜索关键词 <span class="required">*</span></label>
                <input 
                  type="text" 
                  id="keyword" 
                  name="keyword" 
                  class="form-input" 
                  placeholder="例如：machine learning rock strength prediction"
                  required
                >
              </div>
              
              <!-- Content Filter Help -->
              <div class="form-group">
                <label for="content_filter_help" class="form-label">研究方向描述</label>
                <textarea 
                  id="content_help" 
                  name="content_help" 
                  class="form-textarea" 
                  placeholder="描述您的研究方向，帮助AI更精准地筛选相关文献"
                  rows="3"
                ></textarea>
                <p class="form-hint">可选：提供研究方向描述可提高筛选精准度</p>
              </div>

              <!-- LLM Enhancement Toggle -->
              <div class="form-group">
                <label class="llm-toggle-card" for="enable_llm">
                  <input id="enable_llm" name="enable_llm" type="checkbox" checked>
                  <span class="llm-toggle-copy">
                    <strong>启用大模型增强</strong>
                    <small>用于关键词翻译、关键词扩展、相关性筛选和语义评分；关闭后仅使用原始关键词和本地排序。</small>
                  </span>
                </label>
              </div>

              <!-- Source Selection -->
              <div class="form-group">
                <label class="form-label">检索来源</label>
                <div class="source-chip-group" id="source-chips" role="group" aria-label="选择检索来源">
                  <span class="source-loading">加载检索源...</span>
                </div>
                <p class="form-hint">至少选择一个来源；未选将使用服务端默认来源</p>
              </div>
              
              <!-- Year and Impact Factor Row -->
              <div class="form-row">
                <div class="form-group">
                  <label for="ylo" class="form-label">起始年份</label>
                  <input 
                    type="number" 
                    id="ylo" 
                    name="ylo" 
                    class="form-input" 
                    min="1900" 
                    max="2030"
                    value="${defaultYear}"
                  >
                  <p class="form-hint">默认：近5年</p>
                </div>
                
                <div class="form-group">
                  <label for="sciif" class="form-label">最低影响因子 (IF)</label>
                  <input 
                    type="number" 
                    id="sciif" 
                    name="sciif" 
                    class="form-input" 
                    step="0.1" 
                    min="0"
                    value="3"
                  >
                  <p class="form-hint">默认：3.0</p>
                </div>
              </div>
              
              <!-- Optional Filters -->
              <details class="optional-filters">
                <summary class="filters-toggle">
                  <span>更多筛选条件</span>
                  <i class="bi bi-chevron-down toggle-icon"></i>
                </summary>
                <div class="filters-content">
                  <div class="form-row">
                    <div class="form-group">
                      <label for="jci" class="form-label">最低 JCI 分数</label>
                      <input 
                        type="number" 
                        id="jci" 
                        name="jci" 
                        class="form-input" 
                        step="0.01" 
                        min="0"
                      >
                    </div>
                    
                    <div class="form-group">
                      <label for="sci" class="form-label">SCI 分区</label>
                      <select id="sci" name="sci" class="form-input">
                        <option value="">全部</option>
                        <option value="Q1">Q1</option>
                        <option value="Q2">Q2</option>
                        <option value="Q3">Q3</option>
                        <option value="Q4">Q4</option>
                      </select>
                    </div>
                  </div>
                  <div class="form-row">
                    <div class="form-group">
                      <label for="sort_by" class="form-label">排序方式</label>
                      <select id="sort_by" name="sort_by" class="form-input">
                        <option value="relevance" selected>相关性排序（推荐）</option>
                        <option value="impact_factor">影响因子排序</option>
                      </select>
                      <p class="form-hint">相关性会综合关键词、研究方向、标题和摘要进行排序。</p>
                    </div>
                    <div class="form-group">
                      <label for="pdf_sort" class="form-label">PDF 排序</label>
                      <select id="pdf_sort" name="pdf_sort" class="form-input">
                        <option value="">默认排序</option>
                        <option value="pdf_first">有 PDF 优先</option>
                        <option value="pdf_last">无 PDF 优先</option>
                      </select>
                      <p class="form-hint">后端返回和 CSV 保存会按 PDF 可用性排序；结果表也可点击 PDF 列切换。</p>
                    </div>
                  </div>
                </div>
              </details>
              
              <button type="submit" id="submit-btn" class="btn btn-primary">
                <i class="bi bi-search btn-icon"></i>
                <span>开始搜索</span>
              </button>
            </form>
          </div>
        </div>
      </section>

      <!-- Task Status Section -->
      <section id="task-section" class="task-section hidden">
        <div class="container">
          <div class="task-card">
            <div class="task-header">
              <h2 class="section-title"><i class="bi bi-activity"></i> 任务进度</h2>
              <span id="task-id" class="task-id"></span>
            </div>
            
            <div class="progress-container">
              <div class="progress-bar">
                <div id="progress-fill" class="progress-fill"></div>
              </div>
              <div class="progress-info">
                <span id="progress-step" class="progress-step">准备中...</span>
                <span id="progress-percent" class="progress-percent">0%</span>
              </div>
            </div>
            
            <div id="task-status" class="task-status">
              <div class="status-indicator status-queued">
                <span class="status-dot"></span>
                <span id="status-text">等待中</span>
              </div>
            </div>
          </div>
        </div>
      </section>

      <!-- Results Section -->
      <section id="results-section" class="results-section hidden">
        <div class="container">
          <div class="results-card">
            <div class="results-header">
              <h2 class="section-title"><i class="bi bi-table"></i> 搜索结果</h2>
              <div class="results-stats">
                <div class="stat-item">
                  <span class="stat-value" id="total-papers">0</span>
                  <span class="stat-label">总论文数</span>
                </div>
                <div class="stat-item stat-secondary">
                  <span class="stat-value" id="filtered-papers">0</span>
                  <span class="stat-label">筛选后</span>
                </div>
              </div>
            </div>
            
            <div class="download-buttons">
              <button id="download-csv" class="btn btn-secondary">
                <i class="bi bi-download btn-icon"></i>
                <span>下载 CSV</span>
              </button>
              <button id="download-bibtex" class="btn btn-outline">
                <i class="bi bi-file-earmark-text btn-icon"></i>
                <span>导出 BibTeX</span>
              </button>
            </div>
            
            <div class="results-warning">
              <i class="bi bi-info-circle"></i>
              <span>搜索结果保存在云端；连续 2 小时未访问后会自动清理</span>
            </div>

            <div id="query-plan-panel" class="query-plan-panel hidden"></div>
            
            <div id="results-table-container" class="results-table-container">
              <!-- Table will be inserted here -->
            </div>
          </div>
        </div>
      </section>

      <!-- Error Section -->
      <section id="error-section" class="error-section hidden">
        <div class="container">
          <div class="error-card">
            <div class="error-icon">
              <i class="bi bi-exclamation-triangle"></i>
            </div>
            <h3 class="error-title">出错了</h3>
            <p id="error-message" class="error-message"></p>
            <button id="retry-btn" class="btn btn-primary">重试</button>
          </div>
        </div>
      </section>
    `;
  }

  mount() {
    // Form submission
    document.getElementById('search-form')?.addEventListener('submit', (e) => this.handleSubmit(e));
    document.getElementById('download-csv')?.addEventListener('click', () => this.handleDownloadCSV());
    document.getElementById('download-bibtex')?.addEventListener('click', () => this.handleDownloadBibTeX());
    document.getElementById('retry-btn')?.addEventListener('click', () => this.handleRetry());
    document.getElementById('history-toggle')?.addEventListener('click', () => window.toggleSidebar());

    // Dynamically load available sources from server config
    this.loadSources();
  }

  async loadSources() {
    const container = document.getElementById('source-chips');
    if (!container) return;
    try {
      const data = await fetchSources();
      if (!data.sources || data.sources.length === 0) {
        container.innerHTML = '<span class="source-loading">暂无可用检索源</span>';
        return;
      }
      container.innerHTML = data.sources.map((s) => `
        <label class="source-chip">
          <input type="checkbox" name="source_include" value="${s.id}" checked>
          <span>${s.label}</span>
        </label>
      `).join('');
    } catch (e) {
      console.warn('Failed to load sources:', e);
      container.innerHTML = '<span class="source-loading">检索源加载失败</span>';
    }
  }

  async handleSubmit(e) {
    e.preventDefault();

    this.hideAllSections();
    this.setButtonLoading(true);

    const formData = new FormData(document.getElementById('search-form'));
    const params = this.buildRequestParams(formData);
    this.currentSortMode = params.sort_by || 'relevance';

    try {
      const response = await createTask(params);
      const taskId = response.task_id;
      this.currentTaskId = taskId;

      // Add to local history
      historyManager.addTask(taskId, params.keyword, 'queued');
      renderHistoryList();

      // Show task section
      this.showTaskSection(taskId);

      // Reset button immediately - don't block for polling
      this.setButtonLoading(false);

      // Poll for status in background (non-blocking)
      this.pollInBackground(taskId);

    } catch (error) {
      if (this.currentTaskId) {
        historyManager.updateTask(this.currentTaskId, { status: 'failed' });
        renderHistoryList();
      }
      this.showError(error.message);
      this.setButtonLoading(false);
    }
  }

  async pollInBackground(taskId) {
    try {
      const result = await pollTaskStatus(taskId, (status) => {
        // Only update UI if this is still the current task being viewed
        if (this.currentTaskId === taskId) {
          this.handleProgressUpdate(status);
        } else {
          // Still update history even if not viewing
          historyManager.updateTask(taskId, { status: status.status });
          renderHistoryList();
        }
      });

      // Update history
      historyManager.updateTask(taskId, { status: 'completed' });
      renderHistoryList();

      // Show results only if still viewing this task
      if (this.currentTaskId === taskId) {
        this.showResults(result);
      }

    } catch (error) {
      historyManager.updateTask(taskId, { status: 'failed' });
      renderHistoryList();

      // Show error only if still viewing this task
      if (this.currentTaskId === taskId) {
        this.showError(error.message);
      }
    }
  }

  buildRequestParams(formData) {
    const params = {
      keyword: formData.get('keyword'),
      enable_crossref: true,
      enable_llm: formData.get('enable_llm') === 'on',
    };

    const ylo = formData.get('ylo');
    if (ylo) params.ylo = parseInt(ylo, 10);

    const contentHelp = formData.get('content_help');
    if (contentHelp?.trim()) params.content_help = contentHelp.trim();

    const sciif = formData.get('sciif');
    if (sciif) params.sciif = parseFloat(sciif);

    const jci = formData.get('jci');
    if (jci?.trim()) params.jci = parseFloat(jci);

    const sci = formData.get('sci');
    if (sci?.trim()) params.sci = sci;

    const sortBy = formData.get('sort_by');
    if (sortBy?.trim()) params.sort_by = sortBy.trim();

    const pdfSort = formData.get('pdf_sort');
    if (pdfSort === 'pdf_first') {
      params.sort_by = 'has_pdf';
      params.sort_order = 'desc';
    } else if (pdfSort === 'pdf_last') {
      params.sort_by = 'has_pdf';
      params.sort_order = 'asc';
    }

    const selectedSources = formData
      .getAll('source_include')
      .map((source) => String(source).trim())
      .filter(Boolean);
    if (selectedSources.length > 0) params.source_include = selectedSources;

    this.setInitialSortFromParams(params);

    return params;
  }

  setInitialSortFromParams(params) {
    if (params.sort_by === 'has_pdf') {
      this.initialSortColumn = 'has_pdf';
      this.initialSortDirection = params.sort_order === 'asc' ? 'asc' : 'desc';
      return;
    }

    if (params.sort_by === 'impact_factor') {
      this.initialSortColumn = 'if_score';
      this.initialSortDirection = 'desc';
      return;
    }

    this.initialSortColumn = 'relevance_score';
    this.initialSortDirection = 'desc';
  }

  handleProgressUpdate(status) {
    const percent = status.progress?.percent || 0;
    document.getElementById('progress-fill').style.width = `${percent}%`;
    document.getElementById('progress-percent').textContent = `${percent}%`;

    const step = this.cleanStepText(status.progress?.step);
    document.getElementById('progress-step').textContent = step;

    this.updateStatusIndicator(status.status);

    // Update history with real backend status
    if (this.currentTaskId) {
      historyManager.updateTask(this.currentTaskId, { status: status.status });
      renderHistoryList();
    }
  }

  cleanStepText(step) {
    if (!step) return '处理中...';
    return step.replace(/\s*\([^)]*\)\s*/g, ' ').trim();
  }

  updateStatusIndicator(status) {
    const indicator = document.querySelector('#task-status .status-indicator');
    const statusText = document.getElementById('status-text');

    indicator.classList.remove('status-queued', 'status-running', 'status-completed', 'status-failed');

    const statusMap = {
      'pending': ['status-queued', '等待中'],
      'queued': ['status-queued', '等待中'],
      'running': ['status-running', '运行中'],
      'completed': ['status-completed', '已完成'],
      'failed': ['status-failed', '失败']
    };

    const [cls, text] = statusMap[status] || ['status-queued', status];
    indicator.classList.add(cls);
    statusText.textContent = text;
  }

  showTaskSection(taskId) {
    document.getElementById('task-id').textContent = `ID: ${taskId}`;
    document.getElementById('task-section').classList.remove('hidden');
    document.getElementById('task-section').classList.add('fade-in');

    document.getElementById('progress-fill').style.width = '0%';
    document.getElementById('progress-percent').textContent = '0%';
    document.getElementById('progress-step').textContent = '准备中...';
    this.updateStatusIndicator('queued');
  }

  showResults(result) {
    const total = result.result?.total_papers || 0;
    const filtered = result.result?.filtered_papers || 0;

    document.getElementById('total-papers').textContent = total;
    document.getElementById('filtered-papers').textContent = filtered;

    this.currentPapers = result.result?.data || [];
    renderQueryPlanPanel(document.getElementById('query-plan-panel'), result.result?.query_plan);
    this.currentSortColumn = this.initialSortColumn;
    this.currentSortDirection = this.initialSortDirection;
    this.sortPapers();
    this.buildResultsTable();

    document.getElementById('results-section').classList.remove('hidden');
    document.getElementById('results-section').classList.add('fade-in');
    document.getElementById('results-section').scrollIntoView({ behavior: 'smooth' });
  }

  sortPapers() {
    sortPaperRows(this.currentPapers, this.currentSortColumn, this.currentSortDirection);
  }

  handleSort(column) {
    if (this.currentSortColumn === column) {
      this.currentSortDirection = this.currentSortDirection === 'asc' ? 'desc' : 'asc';
    } else {
      this.currentSortColumn = column;
      this.currentSortDirection = defaultSortDirection(column);
    }

    this.sortPapers();
    this.buildResultsTable();
  }

  getSortIndicator(column) {
    if (this.currentSortColumn !== column) return '';
    return this.currentSortDirection === 'asc' ? ' ↑' : ' ↓';
  }

  getAuthorsString(authors) {
    return getAuthorsString(authors);
  }

  getJournalName(paper) {
    return getJournalName(paper);
  }

  getImpactFactor(paper) {
    const ifValue = paper.if_score || paper.sciif || paper.impactFactor || paper.if || paper.IF;
    if (ifValue && !isNaN(parseFloat(ifValue))) {
      return parseFloat(ifValue).toFixed(1);
    }
    return '-';
  }

  getImpactFactorValue(paper) {
    return getImpactFactorValue(paper);
  }

  getRelevanceScore(paper) {
    const score = this.getRelevanceScoreValue(paper);
    return score > 0 ? `${score}` : '-';
  }

  getRelevanceScoreValue(paper) {
    return getRelevanceScoreValue(paper);
  }

  getJournalColor(ifScore) {
    // Gradient from red (high IF) to blue (low IF)
    // High IF (>20): Deep red
    // Medium IF (10-20): Orange-red
    // Medium IF (5-10): Purple
    // Low IF (<5): Blue
    if (ifScore >= 20) {
      return 'background: linear-gradient(135deg, #DC2626, #EF4444); color: white;';
    } else if (ifScore >= 10) {
      return 'background: linear-gradient(135deg, #F97316, #FB923C); color: white;';
    } else if (ifScore >= 5) {
      return 'background: linear-gradient(135deg, #8B5CF6, #A78BFA); color: white;';
    } else if (ifScore >= 3) {
      return 'background: linear-gradient(135deg, #3B82F6, #60A5FA); color: white;';
    } else if (ifScore > 0) {
      return 'background: linear-gradient(135deg, #60A5FA, #93C5FD); color: #1E40AF;';
    } else {
      return 'background: #F3F4F6; color: #6B7280;';
    }
  }

  truncateText(text, maxLength) {
    if (!text) return '';
    if (text.length <= maxLength) return text;
    return text.substring(0, maxLength) + '...';
  }

  truncateAuthors(authors) {
    if (!authors) return '-';
    if (typeof authors === 'string') {
      const parts = authors.split(',');
      if (parts.length > 2) return parts.slice(0, 2).join(', ') + ' et al.';
      return authors;
    }
    if (Array.isArray(authors)) {
      if (authors.length > 2) return authors.slice(0, 2).join(', ') + ' et al.';
      return authors.join(', ');
    }
    return String(authors);
  }

  buildResultsTable() {
    const container = document.getElementById('results-table-container');

    if (!this.currentPapers || this.currentPapers.length === 0) {
      container.innerHTML = `
        <div class="empty-state">
          <i class="bi bi-inboxes"></i>
          <p>暂无结果</p>
        </div>
      `;
      return;
    }

    const table = document.createElement('table');
    table.className = 'results-table';

    table.innerHTML = `
      <thead>
        <tr>
          <th class="sortable" data-column="title">标题${this.getSortIndicator('title')}</th>
          <th class="sortable" data-column="authors">作者${this.getSortIndicator('authors')}</th>
          <th class="sortable" data-column="year">年份${this.getSortIndicator('year')}</th>
          <th class="sortable" data-column="journal">期刊${this.getSortIndicator('journal')}</th>
          <th class="sortable" data-column="relevance_score">相关性${this.getSortIndicator('relevance_score')}</th>
          <th class="sortable" data-column="if_score">IF${this.getSortIndicator('if_score')}</th>
          <th class="sortable" data-column="has_pdf">PDF${this.getSortIndicator('has_pdf')}</th>
        </tr>
      </thead>
      <tbody>
        ${this.currentPapers.map(paper => {
      const ifScore = this.getImpactFactorValue(paper);
      const journalStyle = this.getJournalColor(ifScore);
      const pdfUrl = getPdfUrl(paper);
      return `
          <tr>
            <td class="paper-title">
              ${paper.doi
          ? `<a href="https://doi.org/${paper.doi}" target="_blank" rel="noopener">${escapeHtml(paper.title || 'Untitled')}</a>`
          : escapeHtml(paper.title || 'Untitled')
        }
            </td>
            <td>${escapeHtml(this.truncateAuthors(paper.authors))}</td>
            <td>${paper.year || '-'}</td>
            <td><span class="journal-tag" style="${journalStyle}">${escapeHtml(this.getJournalName(paper)) || '-'}</span></td>
            <td class="relevance-value" title="${escapeHtml(paper.relevance_reason || '')}">${this.getRelevanceScore(paper)}</td>
            <td class="if-value">${this.getImpactFactor(paper)}</td>
            <td class="pdf-cell">
              ${pdfUrl
          ? `<a href="${escapeHtml(pdfUrl)}" target="_blank" rel="noopener" class="pdf-link" title="下载 PDF">
                    <i class="bi bi-file-earmark-pdf"></i>
                  </a>`
          : '<span class="pdf-none">-</span>'
        }
            </td>
          </tr>
        `}).join('')}
      </tbody>
    `;

    container.innerHTML = '';
    container.appendChild(table);

    table.querySelectorAll('th.sortable').forEach(th => {
      th.addEventListener('click', () => this.handleSort(th.dataset.column));
    });
  }

  showError(message) {
    document.getElementById('error-message').textContent = message;
    document.getElementById('error-section').classList.remove('hidden');
    document.getElementById('error-section').classList.add('fade-in');
  }

  hideAllSections() {
    document.getElementById('task-section')?.classList.add('hidden');
    document.getElementById('results-section')?.classList.add('hidden');
    document.getElementById('error-section')?.classList.add('hidden');
  }

  setButtonLoading(isLoading) {
    const btn = document.getElementById('submit-btn');
    if (isLoading) {
      btn.disabled = true;
      btn.innerHTML = '<span class="spinner-border text-primary" aria-hidden="true"></span><span>搜索中...</span>';
      btn.classList.add('btn-loading');
    } else {
      btn.disabled = false;
      btn.innerHTML = `
        <i class="bi bi-search btn-icon"></i>
        <span>开始搜索</span>
      `;
      btn.classList.remove('btn-loading');
    }
  }

  handleDownloadCSV() {
    if (this.currentTaskId) downloadCSV(this.currentTaskId);
  }

  handleDownloadBibTeX() {
    if (this.currentTaskId) downloadBibTeX(this.currentTaskId);
  }

  handleRetry() {
    this.hideAllSections();
    window.scrollTo({ top: 0, behavior: 'smooth' });
  }
}
