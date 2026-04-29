/**
 * ScholarLens API Client
 * Handles all communication with the backend through the proxy
 */

const API_BASE = '';

/**
 * Create a search task
 * @param {Object} params - Search parameters
 * @returns {Promise<Object>} Task creation response
 */
export async function createTask(params) {
    const response = await fetch(`${API_BASE}/tasks`, {
        method: 'POST',
        headers: {
            'Content-Type': 'application/json',
        },
        body: JSON.stringify(params),
    });

    if (!response.ok) {
        const error = await response.text();
        throw new Error(`创建任务失败: ${error}`);
    }

    return response.json();
}

/**
 * Get task status
 * @param {string} taskId - Task ID to check
 * @returns {Promise<Object>} Task status response
 */
export async function getTaskStatus(taskId) {
    const response = await fetch(`${API_BASE}/tasks/${taskId}`);

    if (!response.ok) {
        const error = await response.text();
        throw new Error(`获取任务状态失败: ${error}`);
    }

    return response.json();
}

/**
 * Fetch persisted task history from the backend database.
 * @param {Object} options
 * @param {number} options.limit - Maximum items to return
 * @returns {Promise<Object>} Task history response
 */
export async function fetchTaskHistory({ limit = 50 } = {}) {
    const params = new URLSearchParams({ limit: String(limit) });
    const response = await fetch(`${API_BASE}/tasks?${params.toString()}`);

    if (!response.ok) {
        const error = await response.text();
        throw new Error(`获取历史记录失败: ${error}`);
    }

    return response.json();
}

/**
 * Download CSV results
 * @param {string} taskId - Task ID
 */
export function downloadCSV(taskId) {
    const link = document.createElement('a');
    link.href = `${API_BASE}/tasks/${taskId}/download`;
    link.download = `scholarlens_${taskId}.csv`;
    document.body.appendChild(link);
    link.click();
    document.body.removeChild(link);
}

/**
 * Download BibTeX results
 * @param {string} taskId - Task ID
 */
export function downloadBibTeX(taskId) {
    const link = document.createElement('a');
    link.href = `${API_BASE}/tasks/${taskId}/bibtex`;
    link.download = `scholarlens_${taskId}.bib`;
    document.body.appendChild(link);
    link.click();
    document.body.removeChild(link);
}

/**
 * Check API health
 * @returns {Promise<Object>} Health status
 */
export async function checkHealth() {
    const response = await fetch(`${API_BASE}/health`);

    if (!response.ok) {
        throw new Error('服务不可用');
    }

    return response.json();
}

/**
 * Fetch enabled search sources from server config
 * @returns {Promise<Object>} Sources response { sources: [{id, label}, ...] }
 */
export async function fetchSources() {
    const response = await fetch(`${API_BASE}/sources`);

    if (!response.ok) {
        throw new Error('获取检索源失败');
    }

    return response.json();
}

function adminHeaders(adminKey) {
    return {
        'Content-Type': 'application/json',
        'X-API-Key': adminKey,
    };
}

async function parseAdminResponse(response, fallbackMessage) {
    const raw = await response.text();
    const contentType = response.headers.get('content-type') || '';
    const looksLikeHtml = contentType.includes('text/html') || raw.trimStart().startsWith('<!DOCTYPE html');

    if (looksLikeHtml) {
        throw new Error('管理接口未启用或请求被前端页面接管。请在 config.toml 的 [server] 中设置 admin_enabled = true，重启服务后再试。');
    }

    let payload = {};
    try {
        payload = raw ? JSON.parse(raw) : {};
    } catch {
        payload = { error: { message: raw || fallbackMessage } };
    }

    if (response.status === 401) {
        throw new Error('Admin API Key 缺失或不正确。请粘贴 init-admin 生成的管理员 key。');
    }

    if (response.status === 403) {
        const message = payload.error?.message || '当前 API Key 没有管理员权限，无法管理模型服务商。';
        throw new Error(message);
    }

    if (!response.ok || payload.success === false) {
        const message = payload.error?.message || fallbackMessage;
        throw new Error(message);
    }
    return payload.data || {};
}

export async function listLlmProviders(adminKey) {
    const response = await fetch(`${API_BASE}/api/v1/admin/llm/providers`, {
        headers: { 'X-API-Key': adminKey },
    });
    return parseAdminResponse(response, 'Failed to load LLM providers');
}

export async function saveLlmProvider(adminKey, provider) {
    const response = await fetch(`${API_BASE}/api/v1/admin/llm/providers`, {
        method: 'POST',
        headers: adminHeaders(adminKey),
        body: JSON.stringify(provider),
    });
    return parseAdminResponse(response, 'Failed to save LLM provider');
}

export async function updateLlmProvider(adminKey, name, provider) {
    const response = await fetch(`${API_BASE}/api/v1/admin/llm/providers/${encodeURIComponent(name)}`, {
        method: 'PATCH',
        headers: adminHeaders(adminKey),
        body: JSON.stringify(provider),
    });
    return parseAdminResponse(response, 'Failed to update LLM provider');
}

export async function deleteLlmProvider(adminKey, name) {
    const response = await fetch(`${API_BASE}/api/v1/admin/llm/providers/${encodeURIComponent(name)}`, {
        method: 'DELETE',
        headers: { 'X-API-Key': adminKey },
    });
    return parseAdminResponse(response, 'Failed to delete LLM provider');
}

export async function testLlmProvider(adminKey, name) {
    const response = await fetch(`${API_BASE}/api/v1/admin/llm/providers/${encodeURIComponent(name)}/test`, {
        method: 'POST',
        headers: { 'X-API-Key': adminKey },
    });
    return parseAdminResponse(response, 'Failed to test LLM provider');
}

/**
 * Poll task status until completion
 * @param {string} taskId - Task ID
 * @param {Function} onProgress - Progress callback
 * @param {number} interval - Poll interval in ms
 * @returns {Promise<Object>} Final task result
 */
export async function pollTaskStatus(taskId, onProgress, interval = 2000) {
    return new Promise((resolve, reject) => {
        let consecutiveErrors = 0;
        const maxRetries = 5;

        const poll = async () => {
            try {
                const status = await getTaskStatus(taskId);
                consecutiveErrors = 0; // Reset on success

                if (onProgress) {
                    onProgress(status);
                }

                if (status.status === 'completed') {
                    resolve(status);
                } else if (status.status === 'failed') {
                    reject(new Error(status.error || '任务执行失败'));
                } else {
                    // Continue polling for 'queued' or 'running' status
                    setTimeout(poll, interval);
                }
            } catch (error) {
                consecutiveErrors++;
                console.warn(`Polling error (${consecutiveErrors}/${maxRetries}):`, error.message);

                if (consecutiveErrors >= maxRetries) {
                    reject(new Error('网络连接失败，请检查网络后刷新页面'));
                } else {
                    // Retry after a delay
                    setTimeout(poll, interval * 2);
                }
            }
        };

        poll();
    });
}
