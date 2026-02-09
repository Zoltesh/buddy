/**
 * API helpers and data conversion utilities.
 */

/** Fetch all conversation summaries. */
export async function fetchConversations() {
  const res = await fetch('/api/conversations');
  if (!res.ok) throw new Error('Failed to load conversations');
  return res.json();
}

/** Fetch a single conversation with full message history. */
export async function fetchConversation(id) {
  const res = await fetch(`/api/conversations/${id}`);
  if (!res.ok) throw new Error('Failed to load conversation');
  return res.json();
}

/** Delete a conversation. */
export async function deleteConversation(id) {
  const res = await fetch(`/api/conversations/${id}`, { method: 'DELETE' });
  if (!res.ok) throw new Error('Failed to delete conversation');
}

/** Fetch current system warnings. */
export async function fetchWarnings() {
  const res = await fetch('/api/warnings');
  if (!res.ok) throw new Error('Failed to load warnings');
  return res.json();
}

/** Fetch the current server configuration. */
export async function fetchConfig() {
  const res = await fetch('/api/config');
  if (!res.ok) throw new Error('Failed to load config');
  return res.json();
}

/**
 * Update a config section via PUT.
 * Returns the updated config on success; throws with error details on failure.
 */
async function putConfigSection(section, body) {
  const res = await fetch(`/api/config/${section}`, {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  });
  const data = await res.json();
  if (!res.ok) {
    const err = new Error('Config update failed');
    err.details = data;
    throw err;
  }
  return data;
}

/** Update the chat config (system_prompt). */
export function putConfigChat(chat) {
  return putConfigSection('chat', chat);
}

/** Update the memory config (auto_retrieve, similarity_threshold, auto_retrieve_limit). */
export function putConfigMemory(memory) {
  return putConfigSection('memory', memory);
}

/** Update the models config (chat and embedding provider lists). */
export function putConfigModels(models) {
  return putConfigSection('models', models);
}

/** Update the skills config (read_file, write_file, fetch_url). */
export function putConfigSkills(skills) {
  return putConfigSection('skills', skills);
}

/** Test a provider's connectivity without saving. */
export async function testProvider(entry) {
  const res = await fetch('/api/config/test-provider', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(entry),
  });
  const data = await res.json();
  if (!res.ok) {
    const err = new Error('Validation failed');
    err.details = data;
    throw err;
  }
  return data;
}

/**
 * Convert backend messages to display items.
 * Groups tool_call + tool_result pairs into single blocks.
 *
 * Display item shapes:
 *   { kind: 'text', role, content, timestamp }
 *   { kind: 'tool_call', id, name, arguments, result }
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

/**
 * Format a timestamp as a relative time string.
 */
export function timeAgo(dateStr) {
  const date = new Date(dateStr);
  const now = new Date();
  const seconds = Math.floor((now - date) / 1000);

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
