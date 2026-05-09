import assert from 'node:assert/strict';
import test from 'node:test';

test('admin provider load error keeps setup instructions hidden', async () => {
  const originalFetch = globalThis.fetch;
  globalThis.fetch = async () => new Response(
    JSON.stringify({ error: { message: 'unauthorized' } }),
    { status: 401, headers: { 'content-type': 'application/json' } },
  );

  try {
    const { listLlmProviders } = await import(`./client.js?test=${Date.now()}`);
    await assert.rejects(
      () => listLlmProviders('bad-key'),
      (error) => {
        assert.equal(error.message, '管理员密码缺失或不正确。');
        assert.doesNotMatch(error.message, /init-admin|config\.toml/);
        return true;
      },
    );
  } finally {
    globalThis.fetch = originalFetch;
  }
});
