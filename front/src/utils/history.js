/**
 * History Manager - Local Storage for Query History
 *
 * Strategy: local entries are authoritative for status; server is only used to
 * surface tasks created from other tabs/devices and to drop entries the
 * server has already cleaned up. Hidden-id and "cleared-before" markers
 * prevent server data from re-appearing after the user clears history.
 */

const STORAGE_KEY = 'rustscholar_history';
const HIDDEN_SERVER_KEY = 'rustscholar_hidden_server_history';
const HIDDEN_TIMESTAMP_KEY = 'rustscholar_hidden_server_history_at';
const SERVER_CLEAR_BEFORE_KEY = 'rustscholar_server_history_cleared_before';
const MAX_HISTORY_ITEMS = 50;
const HIDDEN_ID_TTL_MS = 7 * 24 * 60 * 60 * 1000; // hidden ids forgotten after 7 days
const TERMINAL_STATUSES = new Set(['completed', 'failed']);

export class HistoryManager {
  constructor() {
    this.history = this.loadHistory();
    const { ids, timestamps } = this.loadHiddenState();
    this.hiddenServerTaskIds = ids;
    this.hiddenServerTaskTimestamps = timestamps;
    this.serverHistoryClearedBefore = this.loadServerHistoryClearedBefore();
    this.pruneHiddenIds();
  }

  // ---------- persistence ----------
  loadHistory() {
    try {
      const data = localStorage.getItem(STORAGE_KEY);
      const parsed = data ? JSON.parse(data) : [];
      return Array.isArray(parsed) ? parsed : [];
    } catch (e) {
      console.error('Failed to load history:', e);
      return [];
    }
  }

  saveHistory() {
    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(this.history));
    } catch (e) {
      console.error('Failed to save history:', e);
    }
  }

  loadHiddenState() {
    let ids = [];
    let timestamps = {};
    try {
      const rawIds = localStorage.getItem(HIDDEN_SERVER_KEY);
      ids = rawIds ? JSON.parse(rawIds) : [];
      if (!Array.isArray(ids)) ids = [];
    } catch (e) {
      console.error('Failed to load hidden server history:', e);
      ids = [];
    }
    try {
      const rawTs = localStorage.getItem(HIDDEN_TIMESTAMP_KEY);
      const parsed = rawTs ? JSON.parse(rawTs) : {};
      if (parsed && typeof parsed === 'object' && !Array.isArray(parsed)) {
        timestamps = parsed;
      }
    } catch (e) {
      console.error('Failed to load hidden server history timestamps:', e);
      timestamps = {};
    }
    const now = Date.now();
    const idSet = new Set();
    for (const id of ids) {
      if (typeof id !== 'string' || !id) continue;
      idSet.add(id);
      if (!Number.isFinite(timestamps[id])) timestamps[id] = now;
    }
    // strip orphan timestamps
    for (const id of Object.keys(timestamps)) {
      if (!idSet.has(id)) delete timestamps[id];
    }
    return { ids: idSet, timestamps };
  }

  saveHiddenState() {
    try {
      localStorage.setItem(HIDDEN_SERVER_KEY, JSON.stringify([...this.hiddenServerTaskIds]));
      localStorage.setItem(HIDDEN_TIMESTAMP_KEY, JSON.stringify(this.hiddenServerTaskTimestamps));
    } catch (e) {
      console.error('Failed to save hidden server history:', e);
    }
  }

  loadServerHistoryClearedBefore() {
    try {
      const value = Number(localStorage.getItem(SERVER_CLEAR_BEFORE_KEY));
      return Number.isFinite(value) && value > 0 ? value : 0;
    } catch (e) {
      console.error('Failed to load server history clear timestamp:', e);
      return 0;
    }
  }

  saveServerHistoryClearedBefore() {
    try {
      if (this.serverHistoryClearedBefore > 0) {
        localStorage.setItem(SERVER_CLEAR_BEFORE_KEY, String(this.serverHistoryClearedBefore));
      } else {
        localStorage.removeItem(SERVER_CLEAR_BEFORE_KEY);
      }
    } catch (e) {
      console.error('Failed to save server history clear timestamp:', e);
    }
  }

  pruneHiddenIds() {
    const cutoff = Date.now() - HIDDEN_ID_TTL_MS;
    let dirty = false;
    for (const [id, ts] of Object.entries(this.hiddenServerTaskTimestamps)) {
      if (!Number.isFinite(ts) || ts < cutoff) {
        delete this.hiddenServerTaskTimestamps[id];
        this.hiddenServerTaskIds.delete(id);
        dirty = true;
      }
    }
    if (dirty) this.saveHiddenState();
  }

  hideServerId(taskId, when = Date.now()) {
    if (!taskId) return;
    this.hiddenServerTaskIds.add(taskId);
    this.hiddenServerTaskTimestamps[taskId] = when;
  }

  // ---------- mutations ----------
  addTask(taskId, keyword, status = 'queued') {
    if (!taskId) return;
    if (this.hiddenServerTaskIds.delete(taskId)) {
      delete this.hiddenServerTaskTimestamps[taskId];
      this.saveHiddenState();
    }

    const idx = this.history.findIndex((item) => item.taskId === taskId);
    const now = Date.now();
    if (idx >= 0) {
      this.history[idx] = {
        ...this.history[idx],
        keyword: keyword || this.history[idx].keyword,
        status,
        updatedAt: now,
      };
    } else {
      this.history.unshift({
        taskId,
        keyword: keyword || '未命名检索',
        status,
        createdAt: now,
        updatedAt: now,
      });
    }

    if (this.history.length > MAX_HISTORY_ITEMS) {
      this.history = this.history.slice(0, MAX_HISTORY_ITEMS);
    }
    this.saveHistory();
  }

  updateTask(taskId, updates) {
    const item = this.history.find((entry) => entry.taskId === taskId);
    if (!item) return;
    Object.assign(item, updates, { updatedAt: Date.now() });
    this.saveHistory();
  }

  getTask(taskId) {
    return this.history.find((item) => item.taskId === taskId);
  }

  getHistory() {
    return [...this.history];
  }

  removeTask(taskId) {
    if (!taskId) return;
    this.hideServerId(taskId);
    this.history = this.history.filter((item) => item.taskId !== taskId);
    this.saveHistory();
    this.saveHiddenState();
  }

  clearHistory() {
    const now = Date.now();
    this.history.forEach((item) => this.hideServerId(item.taskId, now));
    this.serverHistoryClearedBefore = now;
    this.history = [];
    this.saveHiddenState();
    this.saveServerHistoryClearedBefore();
    this.saveHistory();
  }

  // ---------- merging ----------
  mergeServerHistory(tasks) {
    if (!Array.isArray(tasks)) return;
    this.pruneHiddenIds();

    const knownIds = new Set(this.history.map((item) => item.taskId));
    const seenServerIds = new Set();
    const byId = new Map(this.history.map((item) => [item.taskId, item]));

    for (const task of tasks) {
      const normalized = this.normalizeServerTask(task);
      if (!normalized) continue;
      seenServerIds.add(normalized.taskId);
      if (this.shouldHideServerTask(normalized)) continue;

      const existing = byId.get(normalized.taskId);
      if (existing) {
        // Local is authoritative for terminal status (completed/failed) and keyword.
        // Only fill in fields the local entry is missing or accept newer non-terminal status.
        const merged = { ...existing };
        if (!TERMINAL_STATUSES.has(existing.status) && normalized.status) {
          merged.status = normalized.status;
        }
        if (!existing.keyword || existing.keyword === '未命名检索') {
          merged.keyword = normalized.keyword || merged.keyword;
        }
        if (!existing.createdAt) merged.createdAt = normalized.createdAt;
        merged.updatedAt = Math.max(existing.updatedAt || 0, normalized.updatedAt || 0);
        merged.source = existing.source || normalized.source;
        byId.set(normalized.taskId, merged);
      } else {
        byId.set(normalized.taskId, normalized);
      }
    }

    // Drop local "server-only" entries that no longer appear server-side
    // (server cleanup removed them after 2h inactivity).
    for (const id of knownIds) {
      const item = byId.get(id);
      if (
        item &&
        item.source === 'server' &&
        TERMINAL_STATUSES.has(item.status) &&
        !seenServerIds.has(id)
      ) {
        byId.delete(id);
      }
    }

    this.history = Array.from(byId.values())
      .sort((a, b) => (b.createdAt || 0) - (a.createdAt || 0))
      .slice(0, MAX_HISTORY_ITEMS);
    this.saveHistory();
  }

  normalizeServerTask(task) {
    const taskId = task?.task_id || task?.taskId;
    if (!taskId) return null;
    return {
      taskId,
      keyword: task.keyword || '未命名检索',
      status: task.status || 'pending',
      createdAt: this.normalizeTimestamp(task.created_at ?? task.createdAt),
      updatedAt: this.normalizeTimestamp(task.updated_at ?? task.updatedAt),
      source: 'server',
    };
  }

  shouldHideServerTask(task) {
    if (this.hiddenServerTaskIds.has(task.taskId)) return true;
    return (
      this.serverHistoryClearedBefore > 0 &&
      (task.createdAt || 0) <= this.serverHistoryClearedBefore
    );
  }

  normalizeTimestamp(value) {
    const timestamp = Number(value);
    if (!Number.isFinite(timestamp) || timestamp <= 0) return Date.now();
    return timestamp < 10_000_000_000 ? timestamp * 1000 : timestamp;
  }
}

export const historyManager = new HistoryManager();
