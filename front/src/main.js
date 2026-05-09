/**
 * ScholarLens Frontend - Main Application with Client-Side Routing
 * Multi-page SPA with history sidebar
 */

import { fetchTaskHistory } from './api/client.js';
import { HomePage } from './pages/home.js';
import { ApiPage } from './pages/api.js';
import { SettingsPage } from './pages/settings.js';
import { TaskPage } from './pages/task.js';
import { historyManager } from './utils/history.js';

// Router state
let currentPage = null;
let historyRefreshInFlight = null;
let historyFilterText = '';

/**
 * Simple client-side router
 */
class Router {
    constructor() {
        this.routes = {
            '/': HomePage,
            '/docs': ApiPage,  // Changed from /api to avoid proxy conflict
            '/settings': SettingsPage,
            '/task': TaskPage,
        };

        window.addEventListener('popstate', () => this.handleRoute());
    }

    navigate(path) {
        window.history.pushState({}, '', path);
        this.handleRoute();
    }

    handleRoute() {
        const path = window.location.pathname;
        const app = document.getElementById('app');

        // Check for task detail page
        if (path.startsWith('/task/')) {
            const taskId = path.split('/task/')[1];
            currentPage = new TaskPage(taskId);
        } else if (this.routes[path]) {
            currentPage = new this.routes[path]();
        } else {
            currentPage = new HomePage();
        }

        app.innerHTML = currentPage.render();
        currentPage.mount();
    }
}

// Global router instance
export const router = new Router();

/**
 * Initialize sidebar
 */
function initSidebar() {
    const sidebar = document.getElementById('sidebar');
    const overlay = document.getElementById('sidebar-overlay');
    const closeBtn = document.getElementById('sidebar-close');
    const refreshBtn = document.getElementById('history-refresh');
    const clearBtn = document.getElementById('clear-history');
    const searchInput = document.getElementById('history-search');

    // Toggle sidebar
    window.toggleSidebar = function (show) {
        if (show === undefined) {
            show = !sidebar.classList.contains('open');
        }
        sidebar.classList.toggle('open', show);
        sidebar.setAttribute('aria-hidden', show ? 'false' : 'true');
        overlay.classList.toggle('active', show);
        if (show) {
            refreshHistoryFromServer();
            // Focus search after open animation
            setTimeout(() => searchInput?.focus(), 280);
        }
    };

    closeBtn?.addEventListener('click', () => window.toggleSidebar(false));
    overlay?.addEventListener('click', () => window.toggleSidebar(false));

    refreshBtn?.addEventListener('click', () => {
        refreshBtn.classList.add('is-spinning');
        refreshHistoryFromServer().finally(() => {
            setTimeout(() => refreshBtn.classList.remove('is-spinning'), 320);
        });
    });

    clearBtn?.addEventListener('click', () => {
        if (historyManager.getHistory().length === 0) return;
        const ok = window.confirm('确认清空全部历史？此操作不可撤销。');
        if (!ok) return;
        historyManager.clearHistory();
        renderHistoryList();
    });

    searchInput?.addEventListener('input', (event) => {
        historyFilterText = event.target.value.trim().toLowerCase();
        renderHistoryList();
    });

    // ESC closes the sidebar
    document.addEventListener('keydown', (event) => {
        if (event.key === 'Escape' && sidebar.classList.contains('open')) {
            window.toggleSidebar(false);
        }
    });

    // Render history
    renderHistoryList();
    refreshHistoryFromServer();
}

export async function refreshHistoryFromServer() {
    if (historyRefreshInFlight) return historyRefreshInFlight;

    historyRefreshInFlight = fetchTaskHistory({ limit: 50 })
        .then((data) => {
            historyManager.mergeServerHistory(data.items || []);
            renderHistoryList();
        })
        .catch((error) => {
            console.warn('Failed to refresh server history:', error.message);
        })
        .finally(() => {
            historyRefreshInFlight = null;
        });

    return historyRefreshInFlight;
}

/**
 * Render history list in sidebar
 */
export function renderHistoryList() {
    const container = document.getElementById('history-list');
    if (!container) return;

    const history = historyManager.getHistory();
    const countEl = document.getElementById('history-count');
    if (countEl) countEl.textContent = String(history.length);

    if (history.length === 0) {
        container.innerHTML = `
            <div class="history-empty">
                <i class="bi bi-inbox"></i>
                <p>暂无查询历史</p>
                <small>提交搜索后任务会出现在这里</small>
            </div>
        `;
        return;
    }

    const filtered = historyFilterText
        ? history.filter((item) => (item.keyword || '').toLowerCase().includes(historyFilterText))
        : history;

    if (filtered.length === 0) {
        container.innerHTML = `
            <div class="history-empty">
                <i class="bi bi-search"></i>
                <p>没有匹配的记录</p>
                <small>尝试更换关键词</small>
            </div>
        `;
        return;
    }

    container.innerHTML = filtered.map((item) => `
        <div class="history-item" role="listitem" data-task-id="${escapeAttr(item.taskId)}">
            <a href="/task/${escapeAttr(item.taskId)}" class="history-item-link" data-task-id="${escapeAttr(item.taskId)}">
                <div class="history-item-row">
                    ${statusIconHtml(item.status)}
                    <div class="history-keyword" title="${escapeAttr(item.keyword || '')}">${escapeHtml(item.keyword || '未命名检索')}</div>
                </div>
                <div class="history-meta">
                    <span class="history-status history-status-${item.status}">${getStatusText(item.status)}</span>
                    <span class="history-time" title="${escapeAttr(formatFullTime(item.createdAt))}">${formatTime(item.createdAt)}</span>
                </div>
            </a>
            <button class="history-item-delete" data-action="delete" data-task-id="${escapeAttr(item.taskId)}" aria-label="删除" title="从本地历史删除">
                <i class="bi bi-x"></i>
            </button>
        </div>
    `).join('');

    container.querySelectorAll('.history-item-link').forEach((el) => {
        el.addEventListener('click', (event) => {
            event.preventDefault();
            window.toggleSidebar(false);
            router.navigate(`/task/${el.dataset.taskId}`);
        });
    });

    container.querySelectorAll('button[data-action="delete"]').forEach((btn) => {
        btn.addEventListener('click', (event) => {
            event.preventDefault();
            event.stopPropagation();
            historyManager.removeTask(btn.dataset.taskId);
            renderHistoryList();
        });
    });
}

function statusIconHtml(status) {
    const map = {
        pending: 'hourglass-split',
        queued: 'hourglass-split',
        running: 'arrow-repeat',
        completed: 'check-circle-fill',
        failed: 'exclamation-circle-fill',
    };
    const cls = `history-icon history-icon-${status}${status === 'running' ? ' is-spinning' : ''}`;
    return `<span class="${cls}"><i class="bi bi-${map[status] || 'circle'}"></i></span>`;
}

function escapeAttr(value) {
    if (value == null) return '';
    return String(value).replace(/[&<>"']/g, (ch) => ({
        '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;',
    })[ch]);
}

function formatFullTime(timestamp) {
    if (!timestamp) return '';
    const date = new Date(timestamp);
    return date.toLocaleString('zh-CN', { hour12: false });
}

function getStatusText(status) {
    const statusMap = {
        'pending': '等待中',
        'queued': '等待中',
        'running': '运行中',
        'completed': '已完成',
        'failed': '失败'
    };
    return statusMap[status] || status;
}

function formatTime(timestamp) {
    const date = new Date(timestamp);
    const now = new Date();
    const diffMs = now - date;
    const diffMins = Math.floor(diffMs / 60000);
    const diffHours = Math.floor(diffMs / 3600000);
    const diffDays = Math.floor(diffMs / 86400000);

    if (diffMins < 1) return '刚刚';
    if (diffMins < 60) return `${diffMins}分钟前`;
    if (diffHours < 24) return `${diffHours}小时前`;
    if (diffDays < 7) return `${diffDays}天前`;

    return date.toLocaleDateString('zh-CN');
}

function escapeHtml(text) {
    if (!text) return '';
    const div = document.createElement('div');
    div.textContent = text;
    return div.innerHTML;
}

/**
 * Initialize application
 */
function init() {
    initSidebar();
    router.handleRoute();
}

// Initialize on DOM ready
document.addEventListener('DOMContentLoaded', init);

// Export utilities for pages
export { escapeHtml, formatTime, getStatusText };
