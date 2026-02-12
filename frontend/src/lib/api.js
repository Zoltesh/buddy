/**
 * API helpers and data conversion utilities.
 */

const AUTH_TOKEN_KEY = 'buddy_auth_token';

/** Get the stored auth token, or null. */
export function getAuthToken() {
  return localStorage.getItem(AUTH_TOKEN_KEY);
}

/**
 * Store an auth token.
 * @param {string} token
 */
export function setAuthToken(token) {
  localStorage.setItem(AUTH_TOKEN_KEY, token);
}

/** Clear the stored auth token. */
export function clearAuthToken() {
  localStorage.removeItem(AUTH_TOKEN_KEY);
}

/** Check whether the server requires authentication. */
export async function checkAuthStatus() {
  const res = await fetch('/api/auth/status');
  if (!res.ok) throw new Error('Failed to check auth status');
  return res.json();
}

/**
 * Verify a token with the server. Returns { valid: true } or { valid: false }.
 * @param {string} token
 */
export async function verifyToken(token) {
  const res = await fetch('/api/auth/verify', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ token }),
  });
  if (!res.ok) throw new Error('Failed to verify token');
  return res.json();
}

/**
 * Wrapper around fetch that injects the Authorization header when a token
 * is stored, and handles 401 responses by clearing the token and dispatching
 * a 'buddy-auth-expired' event on window.
 * @param {string} url
 * @param {RequestInit} [options]
 */
export async function authFetch(url, options = {}) {
  const token = getAuthToken();
  if (token) {
    options.headers = {
      ...options.headers,
      'Authorization': `Bearer ${token}`,
    };
  }
  const res = await fetch(url, options);
  if (res.status === 401) {
    clearAuthToken();
    window.dispatchEvent(new CustomEvent('buddy-auth-expired'));
  }
  return res;
}

/** Fetch all conversation summaries. */
export async function fetchConversations() {
  const res = await authFetch('/api/conversations');
  if (!res.ok) throw new Error('Failed to load conversations');
  return res.json();
}

/**
 * Fetch a single conversation with full message history.
 * @param {string} id
 */
export async function fetchConversation(id) {
  const res = await authFetch(`/api/conversations/${id}`);
  if (!res.ok) throw new Error('Failed to load conversation');
  return res.json();
}

/**
 * Delete a conversation.
 * @param {string} id
 */
export async function deleteConversation(id) {
  const res = await authFetch(`/api/conversations/${id}`, { method: 'DELETE' });
  if (!res.ok) throw new Error('Failed to delete conversation');
}

/** Fetch current system warnings. */
export async function fetchWarnings() {
  const res = await authFetch('/api/warnings');
  if (!res.ok) throw new Error('Failed to load warnings');
  return res.json();
}

/** Fetch the current server configuration. */
export async function fetchConfig() {
  const res = await authFetch('/api/config');
  if (!res.ok) throw new Error('Failed to load config');
  return res.json();
}

/**
 * Update a config section via PUT.
 * Returns the updated config on success; throws with error details on failure.
 * @param {string} section
 * @param {object} body
 */
async function putConfigSection(section, body) {
  const res = await authFetch(`/api/config/${section}`, {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  });
  const data = await res.json();
  if (!res.ok) {
    throw Object.assign(new Error('Config update failed'), { details: data });
  }
  return data;
}

/**
 * Update the chat config (system_prompt).
 * @param {object} chat
 */
export function putConfigChat(chat) {
  return putConfigSection('chat', chat);
}

/**
 * Update the memory config (auto_retrieve, similarity_threshold, auto_retrieve_limit).
 * @param {object} memory
 */
export function putConfigMemory(memory) {
  return putConfigSection('memory', memory);
}

/**
 * Update the models config (chat and embedding provider lists).
 * @param {object} models
 */
export function putConfigModels(models) {
  return putConfigSection('models', models);
}

/**
 * Update the skills config (read_file, write_file, fetch_url).
 * @param {object} skills
 */
export function putConfigSkills(skills) {
  return putConfigSection('skills', skills);
}

/** Fetch interface connection status. */
export async function fetchInterfacesStatus() {
  const res = await authFetch('/api/interfaces/status');
  if (!res.ok) throw new Error('Failed to load interfaces status');
  return res.json();
}

/** Update the interfaces config section. */
export function putConfigInterfaces(interfaces) {
  return putConfigSection('interfaces', interfaces);
}

/**
 * Check an interface's connection by validating credentials against the external API.
 * @param {string} name - 'telegram' or 'whatsapp'
 * @returns {Promise<{status: string, detail: string}>}
 */
export async function checkInterfaceConnection(name) {
  const res = await authFetch('/api/interfaces/check', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ interface: name }),
  });
  const data = await res.json();
  if (!res.ok) {
    throw Object.assign(new Error('Connection check failed'), { details: data });
  }
  return data;
}

/**
 * Discover available models from an LM Studio endpoint.
 * @param {string} endpoint
 */
export async function discoverModels(endpoint) {
  const res = await authFetch('/api/config/discover-models', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ endpoint }),
  });
  const data = await res.json();
  if (!res.ok) {
    throw Object.assign(new Error('Discovery failed'), { details: data });
  }
  return data;
}

/**
 * Test a provider's connectivity without saving.
 * @param {object} entry
 */
export async function testProvider(entry) {
  const res = await authFetch('/api/config/test-provider', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(entry),
  });
  const data = await res.json();
  if (!res.ok) {
    throw Object.assign(new Error('Validation failed'), { details: data });
  }
  return data;
}

/**
 * Format an API error into a user-facing message string.
 * Supports validation error responses (with field/message pairs) and plain errors.
 * @param {Error & {details?: any, errors?: Array<{field?: string, message: string}>}} e
 * @param {{includeField?: boolean}} [options]
 */
export function formatApiError(e, { includeField = true } = {}) {
  const errors = e.details?.errors || e.errors;
  if (errors?.length) {
    return errors
      .map(err => includeField && err.field ? `${err.field}: ${err.message}` : err.message)
      .join('; ');
  }
  return e.details?.message || e.message || 'Unknown error';
}

/**
 * Convert backend messages to display items.
 * Groups tool_call + tool_result pairs into single blocks.
 *
 * Display item shapes:
 *   { kind: 'text', role, content, timestamp }
 *   { kind: 'tool_call', id, name, arguments, result }
 * @param {Array<{role: string, content: {type: string, [key: string]: any}, timestamp?: string}>} messages
 */
export function toDisplayItems(messages) {
  const items = [];
  for (let i = 0; i < messages.length; i++) {
    const msg = messages[i];
    if (msg.content.type === 'text') {
      items.push({
        kind: 'text',
        role: msg.role,
        content: msg.content.text,
        timestamp: msg.timestamp,
      });
    } else if (msg.content.type === 'tool_call') {
      const block = {
        kind: 'tool_call',
        id: msg.content.id,
        name: msg.content.name,
        arguments: msg.content.arguments,
        result: null,
      };
      // Pair with the following tool_result if it matches.
      if (
        i + 1 < messages.length &&
        messages[i + 1].content.type === 'tool_result' &&
        messages[i + 1].content.id === msg.content.id
      ) {
        block.result = messages[i + 1].content.content;
        i++;
      }
      items.push(block);
    }
    // Standalone tool_result (shouldn't normally happen) — skip.
  }
  return items;
}

/** Get the current embedder health status. */
export async function getEmbedderHealth() {
  const res = await authFetch('/api/embedder/health');
  if (!res.ok) throw new Error('Failed to get embedder health');
  return res.json();
}

/** Get memory status including count and migration requirement. */
export async function getMemoryStatus() {
  const res = await authFetch('/api/memory/status');
  if (!res.ok) throw new Error('Failed to get memory status');
  return res.json();
}

/** Trigger memory migration (re-embedding with new model). */
export async function migrateMemory() {
  const res = await authFetch('/api/memory/migrate', { method: 'POST' });
  if (!res.ok) {
    const data = await res.json();
    throw Object.assign(new Error('Migration failed'), { details: data });
  }
  return res.json();
}

/**
 * Format a timestamp as a relative time string.
 * @param {string} dateStr
 */
export function timeAgo(dateStr) {
  const date = new Date(dateStr);
  const now = new Date();
  const seconds = Math.floor((now.getTime() - date.getTime()) / 1000);

  if (seconds < 60) return 'just now';
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes}m ago`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}h ago`;
  const days = Math.floor(hours / 24);
  if (days < 30) return `${days}d ago`;
  const months = Math.floor(days / 30);
  if (months < 12) return `${months}mo ago`;
  return `${Math.floor(months / 12)}y ago`;
}
