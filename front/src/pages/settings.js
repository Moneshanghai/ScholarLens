/**
 * Settings page for Web-configurable LLM providers.
 */

import {
  changeAdminKey as updateAdminKey,
  deleteLlmProvider,
  listLlmProviders,
  saveLlmProvider,
  testLlmProvider,
  updateLlmProvider,
} from '../api/client.js';
import { escapeHtml, router } from '../main.js';

const STORAGE_KEY = 'scholarlens_admin_key';

export class SettingsPage {
  constructor() {
    this.providers = [];
    this.adminKey = localStorage.getItem(STORAGE_KEY) || '';
    this.isAuthenticated = false;
    this.editingName = null;
  }

  render() {
    return `
      <header class="header">
        <div class="container header-container">
          <a href="/" class="header-logo">ScholarLens</a>
          <nav class="header-nav">
            <button id="history-toggle" class="btn-history" aria-label="查询历史">
              <i class="bi bi-clock-history"></i>
              <span>历史记录</span>
            </button>
            <a href="/docs" class="header-link"><i class="bi bi-file-earmark-text"></i> <span>API 文档</span></a>
            <span class="header-link header-link-active"><i class="bi bi-gear"></i> <span>模型配置</span></span>
            <a href="/logout" data-logout class="header-link header-link-logout" title="登出"><i class="bi bi-box-arrow-right"></i> <span>登出</span></a>
          </nav>
        </div>
      </header>

      <section class="settings-section">
        <div class="container">
          ${this.isAuthenticated ? this.renderSettingsContent() : this.renderAccessGate()}
        </div>
      </section>
    `;
  }

  renderAccessGate() {
    return `
      <div class="settings-gate-card">
        <div class="settings-gate-icon"><i class="bi bi-shield-lock"></i></div>
        <h1 class="settings-gate-title">设置访问</h1>
        <p class="settings-gate-subtitle">请输入管理员密码以管理模型服务商与系统配置</p>
        <form id="settings-access-form" class="settings-form settings-access-form">
          <div class="form-group">
            <label class="form-label" for="admin-key">管理员密码</label>
            <input id="admin-key" class="form-input" type="password" value="${escapeHtml(this.adminKey)}" autocomplete="current-password" placeholder="输入管理员密码" autofocus>
          </div>
          <div id="settings-message" class="settings-message"></div>
          <button id="load-providers" class="btn btn-primary" type="submit">
            <i class="bi bi-box-arrow-in-right"></i>
            <span>进入设置</span>
          </button>
        </form>
      </div>
    `;
  }

  renderSettingsContent() {
    const editing = Boolean(this.editingName);
    return `
      <div class="settings-page-header">
        <div class="settings-page-icon"><i class="bi bi-gear-fill"></i></div>
        <div class="settings-page-meta">
          <h1 class="settings-page-title">模型服务商配置</h1>
          <p class="settings-page-subtitle">管理 OpenAI 兼容服务商，调整管理员密码及连通性测试</p>
        </div>
      </div>

      <div id="settings-message" class="settings-message"></div>

      <div class="settings-grid settings-grid-top">
        <div class="settings-card">
          <div class="settings-card-header">
            <span class="settings-card-icon"><i class="bi bi-key"></i></span>
            <div class="settings-card-meta">
              <h2 class="settings-card-title">访问凭据</h2>
              <p class="settings-card-subtitle">用于调用管理员接口</p>
            </div>
          </div>
          <div class="settings-card-body">
            <div class="form-group">
              <label class="form-label" for="admin-key">管理员密码</label>
              <input id="admin-key" class="form-input" type="password" value="${escapeHtml(this.adminKey)}" autocomplete="current-password" placeholder="当前管理员密码">
              <p class="form-hint">输入后点击下方"刷新配置"重新加载</p>
            </div>
            <div class="settings-actions">
              <button id="load-providers" class="btn btn-primary" type="button">
                <i class="bi bi-arrow-clockwise"></i>
                <span>刷新配置</span>
              </button>
            </div>
          </div>
        </div>

        <div class="settings-card">
          <div class="settings-card-header">
            <span class="settings-card-icon"><i class="bi bi-cpu"></i></span>
            <div class="settings-card-meta">
              <h2 class="settings-card-title">
                已配置服务商
                <span class="settings-count-badge" id="provider-count">${this.providers.length}</span>
              </h2>
              <p class="settings-card-subtitle">按优先级从低到高调用</p>
            </div>
          </div>
          <div class="settings-card-body">
            <div id="provider-list" class="provider-list">
              ${this.providers.length === 0 ? '<div class="empty-state"><i class="bi bi-inboxes"></i><p>暂无服务商</p><small>使用下方表单添加</small></div>' : ''}
            </div>
          </div>
        </div>
      </div>

      <div class="settings-card">
        <div class="settings-card-header">
          <span class="settings-card-icon"><i class="bi bi-${editing ? 'pencil-square' : 'plus-circle'}"></i></span>
          <div class="settings-card-meta">
            <h2 id="provider-form-title" class="settings-card-title">${editing ? `编辑服务商：${escapeHtml(this.editingName)}` : '新增模型服务商'}</h2>
            <p class="settings-card-subtitle">支持 OpenAI 兼容的 chat_completions 与 responses 接口</p>
          </div>
        </div>
        <div class="settings-card-body">
          <form id="provider-form" class="settings-form">
            <div class="form-row">
              <div class="form-group">
                <label class="form-label" for="provider-name">名称 <span class="required">*</span></label>
                <input id="provider-name" name="name" class="form-input" required placeholder="my-provider">
                <p class="form-hint">建议使用小写英文与连字符</p>
              </div>
              <div class="form-group">
                <label class="form-label" for="provider-interface">接口类型</label>
                <select id="provider-interface" name="interface_type" class="form-input">
                  <option value="chat_completions">/v1/chat/completions</option>
                  <option value="responses">/v1/responses</option>
                </select>
              </div>
            </div>

            <div class="form-group">
              <label class="form-label" for="provider-endpoint">Endpoint <span class="required">*</span></label>
              <input id="provider-endpoint" name="endpoint" class="form-input" required placeholder="https://example.com 或 https://example.com/v1/chat/completions">
              <p class="form-hint">Base URL 会按接口类型自动补全路径</p>
            </div>

            <div class="form-row">
              <div class="form-group">
                <label class="form-label" for="provider-model">模型名称 <span class="required">*</span></label>
                <input id="provider-model" name="model" class="form-input" required placeholder="gpt-4.1-mini">
              </div>
              <div class="form-group">
                <label class="form-label" for="provider-key">API Key</label>
                <input id="provider-key" name="api_key" class="form-input" type="password" placeholder="编辑时留空 = 保留旧 key">
              </div>
            </div>

            <div class="form-row">
              <div class="form-group">
                <label class="form-label" for="provider-order">优先级</label>
                <input id="provider-order" name="order" class="form-input" type="number" value="100">
                <p class="form-hint">数值越小优先级越高</p>
              </div>
              <div class="form-group">
                <label class="form-label">状态</label>
                <label class="settings-toggle">
                  <input id="provider-enabled" name="enabled" type="checkbox" checked>
                  <span>启用此服务商</span>
                </label>
              </div>
            </div>

            <div class="settings-actions">
              <button class="btn btn-primary" type="submit">
                <i class="bi bi-check2"></i>
                <span>保存服务商</span>
              </button>
              <button class="btn btn-secondary" type="submit" data-test-after-save="true">
                <i class="bi bi-wifi"></i>
                <span>保存并测试连通性</span>
              </button>
              <button id="cancel-provider-edit" class="btn btn-outline ${editing ? '' : 'hidden'}" type="button">
                <i class="bi bi-x-lg"></i>
                <span>取消编辑</span>
              </button>
            </div>
          </form>
        </div>
      </div>

      <div class="settings-card">
        <div class="settings-card-header">
          <span class="settings-card-icon settings-card-icon-warn"><i class="bi bi-shield-lock"></i></span>
          <div class="settings-card-meta">
            <h2 class="settings-card-title">安全 · 修改管理员密码</h2>
            <p class="settings-card-subtitle">保存后旧密码立即失效，请妥善保管新密码</p>
          </div>
        </div>
        <div class="settings-card-body">
          <form id="admin-key-form" class="settings-form">
            <div class="form-row">
              <div class="form-group">
                <label class="form-label" for="new-admin-key">新管理员密码</label>
                <input id="new-admin-key" name="new_admin_key" class="form-input" type="password" autocomplete="new-password" placeholder="至少 6 位，不含空格">
              </div>
              <div class="form-group">
                <label class="form-label" for="new-admin-key-confirm">再次输入新密码</label>
                <input id="new-admin-key-confirm" name="new_admin_key_confirm" class="form-input" type="password" autocomplete="new-password" placeholder="再输入一次以确认">
              </div>
            </div>
            <div class="settings-actions">
              <button class="btn btn-secondary" type="submit">
                <i class="bi bi-shield-check"></i>
                <span>保存新密码</span>
              </button>
            </div>
          </form>
        </div>
      </div>
    `;
  }

  mount() {
    window.toggleSidebar?.(false);
    document.querySelectorAll('a[href^="/"]').forEach((link) => {
      if (link.dataset.logout !== undefined) return;
      link.addEventListener('click', (event) => {
        event.preventDefault();
        router.navigate(link.getAttribute('href'));
      });
    });
    document.getElementById('admin-key')?.addEventListener('input', (event) => {
      this.adminKey = event.target.value.trim();
    });
    document.getElementById('settings-access-form')?.addEventListener('submit', (event) => {
      event.preventDefault();
      this.loadProviders();
    });
    document.getElementById('load-providers')?.addEventListener('click', (event) => {
      if (event.currentTarget?.form?.id === 'settings-access-form') return;
      this.loadProviders();
    });
    document.getElementById('admin-key-form')?.addEventListener('submit', (event) => this.changeAdminKey(event));
    document.getElementById('provider-form')?.addEventListener('submit', (event) => this.saveProvider(event));
    document.getElementById('cancel-provider-edit')?.addEventListener('click', () => this.resetProviderForm());
  }

  async loadProviders() {
    if (!this.requireKey()) return;
    this.setMessage('正在加载服务商配置...');
    try {
      const data = await listLlmProviders(this.adminKey);
      const wasAuthenticated = this.isAuthenticated;
      this.providers = data.providers || [];
      this.isAuthenticated = true;
      localStorage.setItem(STORAGE_KEY, this.adminKey);
      if (!wasAuthenticated) {
        this.refreshPage();
      }
      this.renderProviderList();
      this.setMessage(`已加载 ${this.providers.length} 个服务商。`);
    } catch (error) {
      this.setMessage(error.message, true);
    }
  }

  refreshPage() {
    const app = document.getElementById('app');
    if (!app) return;
    app.innerHTML = this.render();
    this.mount();
  }

  async changeAdminKey(event) {
    event.preventDefault();
    if (!this.requireKey()) return;

    const formData = new FormData(event.target);
    const newKey = String(formData.get('new_admin_key') || '').trim();
    const confirmKey = String(formData.get('new_admin_key_confirm') || '').trim();

    if (newKey.length < 6) {
      this.setMessage('新管理员密码至少需要 6 位。', true);
      return;
    }
    if (/\s/.test(newKey)) {
      this.setMessage('新 Admin API Key 不能包含空格或换行。', true);
      return;
    }
    if (newKey !== confirmKey) {
      this.setMessage('两次输入的新 Admin API Key 不一致。', true);
      return;
    }
    if (!window.confirm('确认保存新的 Admin API Key？保存后旧 key 会立即失效。')) {
      return;
    }

    try {
      await updateAdminKey(this.adminKey, this.adminKey, newKey);
      this.adminKey = newKey;
      localStorage.setItem(STORAGE_KEY, newKey);
      const adminKeyInput = document.getElementById('admin-key');
      if (adminKeyInput) adminKeyInput.value = newKey;
      event.target.reset();
      this.setMessage('管理员密码已更新，旧密码已失效。');
    } catch (error) {
      this.setMessage(error.message, true);
    }
  }

  async saveProvider(event) {
    event.preventDefault();
    if (!this.requireKey()) return;
    const formData = new FormData(event.target);
    const apiKey = String(formData.get('api_key') || '').trim();
    const testAfterSave = event.submitter?.dataset?.testAfterSave === 'true';
    const provider = {
      name: String(formData.get('name') || '').trim(),
      enabled: formData.get('enabled') === 'on',
      interface_type: String(formData.get('interface_type') || 'chat_completions'),
      endpoint: String(formData.get('endpoint') || '').trim(),
      model: String(formData.get('model') || '').trim(),
      api_key: apiKey || null,
      order: Number.parseInt(formData.get('order') || '100', 10),
    };

    try {
      if (this.editingName) {
        await updateLlmProvider(this.adminKey, this.editingName, provider);
      } else {
        await saveLlmProvider(this.adminKey, provider);
      }
      this.setMessage(`已保存服务商“${provider.name}”，运行时配置已重新加载。`);
      this.resetProviderForm();
      await this.loadProviders();
      if (testAfterSave) {
        await this.testProvider(provider.name);
      }
    } catch (error) {
      this.setMessage(error.message, true);
    }
  }

  renderProviderList() {
    const container = document.getElementById('provider-list');
    if (!container) return;

    const countEl = document.getElementById('provider-count');
    if (countEl) countEl.textContent = String(this.providers.length);

    if (this.providers.length === 0) {
      container.innerHTML = `
        <div class="empty-state">
          <i class="bi bi-inboxes"></i>
          <p>暂无服务商</p>
          <small>使用下方表单添加</small>
        </div>
      `;
      return;
    }

    const sorted = [...this.providers].sort((a, b) => (a.order || 0) - (b.order || 0));

    container.innerHTML = sorted.map((provider) => {
      const idx = this.providers.indexOf(provider);
      const enabled = provider.enabled;
      const hasKey = provider.api_key_set;
      return `
        <article class="provider-card${enabled ? '' : ' provider-card-disabled'}">
          <div class="provider-card-main">
            <div class="provider-card-head">
              <h3 class="provider-name">${escapeHtml(provider.name)}</h3>
              <span class="provider-badge ${enabled ? 'provider-badge-on' : 'provider-badge-off'}">
                <i class="bi bi-${enabled ? 'check-circle-fill' : 'pause-circle'}"></i>
                ${enabled ? '已启用' : '已禁用'}
              </span>
              <span class="provider-badge provider-badge-priority" title="优先级">
                <i class="bi bi-sort-numeric-down"></i> ${provider.order}
              </span>
            </div>
            <div class="provider-meta-row">
              <span class="provider-meta-item"><i class="bi bi-cpu"></i> ${escapeHtml(provider.model)}</span>
              <span class="provider-meta-item"><i class="bi bi-diagram-3"></i> ${escapeHtml(provider.interface_type)}</span>
              <span class="provider-meta-item ${hasKey ? '' : 'provider-meta-warn'}">
                <i class="bi bi-key${hasKey ? '-fill' : ''}"></i> API Key ${hasKey ? '已设置' : '缺失'}
              </span>
            </div>
            <code class="provider-endpoint" title="${escapeHtml(provider.endpoint)}">${escapeHtml(provider.endpoint)}</code>
          </div>
          <div class="provider-actions">
            <button class="btn btn-secondary btn-small" type="button" data-action="edit" data-index="${idx}">
              <i class="bi bi-pencil"></i> 编辑
            </button>
            <button class="btn btn-secondary btn-small" type="button" data-action="test" data-index="${idx}">
              <i class="bi bi-wifi"></i> 测试
            </button>
            <button class="btn btn-danger btn-small" type="button" data-action="delete" data-index="${idx}">
              <i class="bi bi-trash3"></i> 删除
            </button>
          </div>
        </article>
      `;
    }).join('');

    container.querySelectorAll('button[data-action]').forEach((button) => {
      button.addEventListener('click', () => this.handleProviderAction(button.dataset.action, Number(button.dataset.index)));
    });
  }

  async handleProviderAction(action, index) {
    const provider = this.providers[index];
    if (!provider) return;

    if (action === 'edit') {
      this.editProvider(provider);
      return;
    }

    if (action === 'test') {
      await this.testProvider(provider.name);
      return;
    }

    if (action === 'delete') {
      const name = provider.name;
      if (!window.confirm(`确认删除服务商“${name}”？`)) return;
      try {
        await deleteLlmProvider(this.adminKey, name);
        this.setMessage(`已删除服务商“${name}”，运行时配置已重新加载。`);
        await this.loadProviders();
      } catch (error) {
        this.setMessage(error.message, true);
      }
    }
  }

  editProvider(provider) {
    this.editingName = provider.name;
    const nameInput = document.getElementById('provider-name');
    nameInput.value = provider.name;
    nameInput.readOnly = true;
    nameInput.classList.add('form-input-readonly');
    document.getElementById('provider-interface').value = provider.interface_type;
    document.getElementById('provider-endpoint').value = provider.endpoint;
    document.getElementById('provider-model').value = provider.model;
    document.getElementById('provider-order').value = provider.order;
    document.getElementById('provider-enabled').checked = provider.enabled;
    document.getElementById('provider-key').value = '';
    document.getElementById('provider-form-title').textContent = `编辑服务商：${provider.name}`;
    document.getElementById('cancel-provider-edit')?.classList.remove('hidden');
    document.getElementById('provider-form')?.scrollIntoView({ behavior: 'smooth', block: 'start' });
    this.setMessage(`已载入“${provider.name}”到编辑表单。API Key 留空会保留旧 key。`);
  }

  resetProviderForm() {
    const form = document.getElementById('provider-form');
    form?.reset();
    const nameInput = document.getElementById('provider-name');
    if (nameInput) {
      nameInput.readOnly = false;
      nameInput.classList.remove('form-input-readonly');
    }
    document.getElementById('provider-enabled').checked = true;
    document.getElementById('provider-order').value = '100';
    document.getElementById('provider-form-title').textContent = '新增模型服务商';
    document.getElementById('cancel-provider-edit')?.classList.add('hidden');
    this.editingName = null;
  }

  async testProvider(name) {
    this.setMessage(`正在测试“${name}”连通性...`);
    try {
      const data = await testLlmProvider(this.adminKey, name);
      this.setMessage(`连通性测试成功：${data.output}`);
    } catch (error) {
      this.setMessage(error.message, true);
    }
  }

  requireKey() {
    if (this.adminKey) return true;
    this.setMessage('请先输入管理员密码。', true);
    return false;
  }

  setMessage(message, isError = false) {
    const el = document.getElementById('settings-message');
    if (!el) return;
    el.textContent = message || '';
    el.classList.toggle('settings-message-error', Boolean(isError));
  }
}
