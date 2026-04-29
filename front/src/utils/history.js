/**
 * History Manager - Local Storage for Query History
 */

const STORAGE_KEY = 'rustscholar_history';
const HIDDEN_SERVER_KEY = 'rustscholar_hidden_server_history';
const SERVER_CLEAR_BEFORE_KEY = 'rustscholar_server_history_cleared_before';
const MAX_HISTORY_ITEMS = 50;

export class HistoryManager {
    constructor() {
        this.history = this.loadHistory();
        this.hiddenServerTaskIds = this.loadHiddenServerTaskIds();
        this.serverHistoryClearedBefore = this.loadServerHistoryClearedBefore();
    }

    loadHistory() {
        try {
            const data = localStorage.getItem(STORAGE_KEY);
            return data ? JSON.parse(data) : [];
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

    loadHiddenServerTaskIds() {
        try {
            const data = localStorage.getItem(HIDDEN_SERVER_KEY);
            const ids = data ? JSON.parse(data) : [];
            return new Set(Array.isArray(ids) ? ids : []);
        } catch (e) {
            console.error('Failed to load hidden server history:', e);
            return new Set();
        }
    }

    saveHiddenServerTaskIds() {
        try {
            localStorage.setItem(HIDDEN_SERVER_KEY, JSON.stringify([...this.hiddenServerTaskIds]));
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
                localStorage.setItem(
                    SERVER_CLEAR_BEFORE_KEY,
                    String(this.serverHistoryClearedBefore)
                );
            } else {
                localStorage.removeItem(SERVER_CLEAR_BEFORE_KEY);
            }
        } catch (e) {
            console.error('Failed to save server history clear timestamp:', e);
        }
    }

    addTask(taskId, keyword, status = 'queued') {
        this.hiddenServerTaskIds.delete(taskId);
        this.saveHiddenServerTaskIds();

        // Check if task already exists
        const existing = this.history.findIndex(item => item.taskId === taskId);
        if (existing >= 0) {
            this.history[existing].status = status;
            this.history[existing].updatedAt = Date.now();
        } else {
            this.history.unshift({
                taskId,
                keyword,
                status,
                createdAt: Date.now(),
                updatedAt: Date.now(),
            });
        }

        // Limit history size
        if (this.history.length > MAX_HISTORY_ITEMS) {
            this.history = this.history.slice(0, MAX_HISTORY_ITEMS);
        }

        this.saveHistory();
    }

    updateTask(taskId, updates) {
        const item = this.history.find(item => item.taskId === taskId);
        if (item) {
            Object.assign(item, updates, { updatedAt: Date.now() });
            this.saveHistory();
        }
    }

    getTask(taskId) {
        return this.history.find(item => item.taskId === taskId);
    }

    getHistory() {
        return this.history;
    }

    mergeServerHistory(tasks) {
        if (!Array.isArray(tasks) || tasks.length === 0) return;

        const byId = new Map(this.history.map((item) => [item.taskId, item]));

        for (const task of tasks) {
            const normalized = this.normalizeServerTask(task);
            if (!normalized) continue;
            if (this.shouldHideServerTask(normalized)) continue;

            const existing = byId.get(normalized.taskId);
            byId.set(normalized.taskId, existing ? { ...existing, ...normalized } : normalized);
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
            task.source === 'server' &&
            (task.createdAt || 0) <= this.serverHistoryClearedBefore
        );
    }

    normalizeTimestamp(value) {
        const timestamp = Number(value);
        if (!Number.isFinite(timestamp) || timestamp <= 0) return Date.now();
        return timestamp < 10_000_000_000 ? timestamp * 1000 : timestamp;
    }

    clearHistory() {
        this.history.forEach((item) => {
            if (item.taskId) this.hiddenServerTaskIds.add(item.taskId);
        });
        this.serverHistoryClearedBefore = Date.now();
        this.history = [];
        this.saveHiddenServerTaskIds();
        this.saveServerHistoryClearedBefore();
        this.saveHistory();
    }

    removeTask(taskId) {
        this.hiddenServerTaskIds.add(taskId);
        this.saveHiddenServerTaskIds();
        this.history = this.history.filter(item => item.taskId !== taskId);
        this.saveHistory();
    }
}

export const historyManager = new HistoryManager();
