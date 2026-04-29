/**
 * Settings page for Web-configurable LLM providers.
 */

import {
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
    this.editingName = null;
  }

  render() {
    return `
      <header class="header">
        <div class="container header-container">
          <a href="/" class="header-logo">ScholarLens</a>
          <a href="/docs" class="header-link"><i class="bi bi-file-earmark-text"></i> API 文档</a>
          <span class="header-link header-link-active"><i class="bi bi-gear"></i> 模型配置</span>
        </div>
      </header>

      <section class="api-section">
        <div class="container">
          <div class="api-card settings-card">
            <h1 class="api-title">模型服务商配置</h1>
            <p class="api-intro">在这里配置 OpenAI 兼容模型服务商。配置会保存到本地 SQLite，保存后立即生效，不需要重启。</p>

            <div class="settings-grid">
              <div class="api-block">
                <h2 class="api-heading">管理员访问</h2>
                <p class="settings-help">
                  Admin API Key 是后端管理员密钥，用来保护模型服务商配置，避免任何人都能修改你的模型 API Key。
                  第一次使用请在服务器终端运行：
                </p>
                <div class="code-block settings-command"><code>cargo run -- init-admin --name Admin</code></div>
                <p class="form-hint">
                  同时确保 <code>config.toml</code> 里 <code>[server].admin_enabled = true</code>，然后把命令输出的 key 粘贴到下面。
                </p>
                <label class="form-label" for="admin-key">Admin API Key（X-API-Key）</label>
                <input id="admin-key" class="form-input" type="password" value="${escapeHtml(this.adminKey)}" placeholder="粘贴 init-admin 输出的管理员 key">
                <p class="form-hint">只保存在当前浏览器 localStorage，不会提交到代码仓库。</p>
                <button id="load-providers" class="btn btn-primary">加载当前配置</button>
              </div>

              <div class="api-block">
                <h2 id="provider-form-title" class="api-heading">新增模型服务商</h2>
                <form id="provider-form" class="settings-form">
                  <label class="form-label" for="provider-name">名称</label>
                  <input id="provider-name" name="name" class="form-input" required placeholder="my-provider">

                  <label class="form-label" for="provider-interface">接口类型</label>
                  <select id="provider-interface" name="interface_type" class="form-input">
                    <option value="chat_completions">/v1/chat/completions</option>
                    <option value="responses">/v1/responses</option>
                  </select>

                  <label class="form-label" for="provider-endpoint">Endpoint</label>
                  <input id="provider-endpoint" name="endpoint" class="form-input" required placeholder="https://example.com 或 https://example.com/v1/chat/completions">
                  <p class="form-hint">可以填写 Base URL，系统会按接口类型自动补全 /v1/chat/completions 或 /v1/responses。</p>

                  <label class="form-label" for="provider-model">模型名称</label>
                  <input id="provider-model" name="model" class="form-input" required placeholder="gpt-4.1-mini">

                  <label class="form-label" for="provider-key">API Key</label>
                  <input id="provider-key" name="api_key" class="form-input" type="password" placeholder="更新已有服务商时留空 = 保留旧 key">

                  <div class="form-row">
                    <div class="form-group">
                      <label class="form-label" for="provider-order">优先级</label>
                      <input id="provider-order" name="order" class="form-input" type="number" value="100">
                    </div>
                    <label class="settings-toggle">
                      <input id="provider-enabled" name="enabled" type="checkbox" checked>
                      <span>启用</span>
                    </label>
                  </div>

                  <div class="settings-actions">
                    <button class="btn btn-primary" type="submit">保存服务商</button>
                    <button class="btn btn-secondary" type="submit" data-test-after-save="true">
                      <i class="bi bi-wifi"></i> 保存并测试连通性
                    </button>
                    <button id="cancel-provider-edit" class="btn btn-outline hidden" type="button">取消编辑</button>
                  </div>
                </form>
              </div>
            </div>

            <div class="api-block">
              <h2 class="api-heading">已配置服务商</h2>
              <div id="settings-message" class="settings-message"></div>
              <div id="provider-list" class="provider-list">
                <p class="form-hint">点击“加载当前配置”查看已有服务商；每个服务商卡片右侧都有“测试连通性”按钮。</p>
              </div>
            </div>
          </div>
        </div>
      </section>
    `;
  }

  mount() {
    window.toggleSidebar?.(false);
    document.querySelectorAll('a[href^="/"]').forEach((link) => {
      link.addEventListener('click', (event) => {
        event.preventDefault();
        router.navigate(link.getAttribute('href'));
      });
    });
    document.getElementById('admin-key')?.addEventListener('input', (event) => {
      this.adminKey = event.target.value.trim();
      localStorage.setItem(STORAGE_KEY, this.adminKey);
    });
    document.getElementById('load-providers')?.addEventListener('click', () => this.loadProviders());
    document.getElementById('provider-form')?.addEventListener('submit', (event) => this.saveProvider(event));
    document.getElementById('cancel-provider-edit')?.addEventListener('click', () => this.resetProviderForm());

    if (this.adminKey) this.loadProviders();
  }

  async loadProviders() {
    if (!this.requireKey()) return;
    this.setMessage('正在加载服务商配置...');
    try {
      const data = await listLlmProviders(this.adminKey);
      this.providers = data.providers || [];
      this.renderProviderList();
      this.setMessage(`已加载 ${this.providers.length} 个服务商。`);
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
    if (this.providers.length === 0) {
      container.innerHTML = '<p class="form-hint">还没有配置服务商。</p>';
      return;
    }

    container.innerHTML = this.providers.map((provider, index) => `
      <article class="provider-card">
        <div>
          <h3>${escapeHtml(provider.name)}</h3>
          <p>${escapeHtml(provider.model)} · ${escapeHtml(provider.interface_type)} · 优先级 ${provider.order}</p>
          <code>${escapeHtml(provider.endpoint)}</code>
          <p class="form-hint">${provider.enabled ? '已启用' : '已禁用'} · API key ${provider.api_key_set ? '已设置' : '缺失'}</p>
        </div>
        <div class="provider-actions">
          <button class="btn btn-secondary" type="button" data-action="edit" data-index="${index}">编辑</button>
          <button class="btn btn-secondary" type="button" data-action="test" data-index="${index}">
            <i class="bi bi-wifi"></i> 测试连通性
          </button>
          <button class="btn btn-danger" type="button" data-action="delete" data-index="${index}">删除</button>
        </div>
      </article>
    `).join('');

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
    this.setMessage('请先粘贴 Admin API Key。可通过 cargo run -- init-admin --name Admin 生成。', true);
    return false;
  }

  setMessage(message, isError = false) {
    const el = document.getElementById('settings-message');
    if (!el) return;
    el.textContent = message || '';
    el.classList.toggle('settings-message-error', Boolean(isError));
  }
}
