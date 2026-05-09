import assert from 'node:assert/strict';
import test from 'node:test';

function createLocalStorage(initial = {}) {
  const store = new Map(Object.entries(initial));
  return {
    getItem: (key) => store.get(key) ?? null,
    setItem: (key, value) => store.set(key, String(value)),
    removeItem: (key) => store.delete(key),
    clear: () => store.clear(),
  };
}

function escapeText(value) {
  return String(value)
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;')
    .replaceAll("'", '&#039;');
}

function installBrowserStubs(localStorageInitial = {}) {
  globalThis.localStorage = createLocalStorage(localStorageInitial);
  globalThis.window = {
    addEventListener() {},
    history: { pushState() {} },
    location: { pathname: '/settings' },
    toggleSidebar() {},
  };
  globalThis.document = {
    addEventListener() {},
    body: {
      appendChild() {},
      removeChild() {},
    },
    createElement() {
      let text = '';
      return {
        set textContent(value) {
          text = String(value);
        },
        get innerHTML() {
          return escapeText(text);
        },
      };
    },
    getElementById() {
      return null;
    },
    querySelectorAll() {
      return [];
    },
  };
}

async function importSettingsPage() {
  const moduleId = `./settings.js?test=${Date.now()}-${Math.random()}`;
  return import(moduleId);
}

test('settings page hides provider configuration and setup instructions before admin key is verified', async () => {
  installBrowserStubs({ scholarlens_admin_key: 'stored-admin-key' });
  const { SettingsPage } = await importSettingsPage();

  const html = new SettingsPage().render();

  assert.match(html, /type="password"/);
  assert.doesNotMatch(html, /在这里配置 OpenAI 兼容模型服务商/);
  assert.doesNotMatch(html, /Admin API Key 是后端管理员密钥/);
  assert.doesNotMatch(html, /cargo run -- init-admin/);
  assert.doesNotMatch(html, /新增模型服务商/);
  assert.doesNotMatch(html, /已配置服务商/);
});

test('authenticated settings page keeps verbose setup instructions out of the UI', async () => {
  installBrowserStubs();
  const { SettingsPage } = await importSettingsPage();
  const page = new SettingsPage();
  page.isAuthenticated = true;

  const html = page.render();

  assert.match(html, /新增模型服务商/);
  assert.match(html, /已配置服务商/);
  assert.doesNotMatch(html, /在这里配置 OpenAI 兼容模型服务商/);
  assert.doesNotMatch(html, /Admin API Key 是后端管理员密钥/);
  assert.doesNotMatch(html, /cargo run -- init-admin/);
});
