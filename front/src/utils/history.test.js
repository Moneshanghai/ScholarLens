import assert from 'node:assert/strict';
import test from 'node:test';

function createLocalStorage() {
  const store = new Map();
  return {
    getItem: (key) => store.get(key) ?? null,
    setItem: (key, value) => store.set(key, String(value)),
    removeItem: (key) => store.delete(key),
    clear: () => store.clear(),
  };
}

test('mergeServerHistory keeps server tasks visible and updates duplicates', async () => {
  globalThis.localStorage = createLocalStorage();
  const { HistoryManager } = await import(`./history.js?test=${Date.now()}`);
  const manager = new HistoryManager();

  manager.addTask('local-1', 'local query', 'running');
  manager.mergeServerHistory([
    {
      task_id: 'server-1',
      keyword: 'older server query',
      status: 'completed',
      created_at: 1_700_000_000_000,
      updated_at: 1_700_000_060_000,
    },
    {
      task_id: 'local-1',
      keyword: 'local query',
      status: 'completed',
      created_at: 1_700_000_120_000,
      updated_at: 1_700_000_180_000,
    },
  ]);

  const history = manager.getHistory();
  assert.deepEqual(history.map((item) => item.taskId).sort(), ['local-1', 'server-1']);
  assert.equal(manager.getTask('local-1').status, 'completed');
  assert.equal(manager.getTask('server-1').keyword, 'older server query');
});
test('clearHistory hides previously synced server tasks from future merges', async () => {
  globalThis.localStorage = createLocalStorage();
  const { HistoryManager } = await import(`./history.js?test=${Date.now()}`);
  const manager = new HistoryManager();

  manager.mergeServerHistory([
    {
      task_id: 'server-cleared-1',
      keyword: 'old server query',
      status: 'completed',
      created_at: 1_700_000_000_000,
      updated_at: 1_700_000_060_000,
    },
  ]);
  assert.equal(manager.getHistory().length, 1);

  manager.clearHistory();
  manager.mergeServerHistory([
    {
      task_id: 'server-cleared-1',
      keyword: 'old server query',
      status: 'completed',
      created_at: 1_700_000_000_000,
      updated_at: 1_700_000_060_000,
    },
  ]);

  assert.deepEqual(manager.getHistory(), []);
});

test('clearHistory hides older server tasks even when they were not loaded yet', async () => {
  globalThis.localStorage = createLocalStorage();
  const originalNow = Date.now;
  Date.now = () => 1_800_000_000_000;
  try {
    const { HistoryManager } = await import(`./history.js?test=${Date.now()}-cutoff`);
    const manager = new HistoryManager();

    manager.clearHistory();
    manager.mergeServerHistory([
      {
        task_id: 'server-not-loaded-before-clear',
        keyword: 'old server query',
        status: 'completed',
        created_at: 1_700_000_000_000,
        updated_at: 1_700_000_060_000,
      },
      {
        task_id: 'server-after-clear',
        keyword: 'new server query',
        status: 'completed',
        created_at: 1_800_000_001_000,
        updated_at: 1_800_000_060_000,
      },
    ]);

    assert.deepEqual(manager.getHistory().map((item) => item.taskId), ['server-after-clear']);
  } finally {
    Date.now = originalNow;
  }
});

