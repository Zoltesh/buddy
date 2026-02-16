<script>
  import { onMount } from 'svelte';
  import { marked } from 'marked';
  import DOMPurify from 'dompurify';
  import {
    fetchConversation,
    fetchWarnings,
    toDisplayItems,
    authFetch,
  } from './api.js';
  import ToolCallBlock from './ToolCallBlock.svelte';

  marked.setOptions({ breaks: true, gfm: true });

  let {
    activeConversationId = null,
    onConversationCreated,
    onReloadConversations,
  } = $props();

  let conversationId = $state(null);
  let conversationSource = $state(null);
  let displayItems = $state([]);
  let inputText = $state('');
  let isStreaming = $state(false);
  let messagesContainer;

  // Warning banner state
  let warnings = $state([]);
  let dismissedCodes = $state(new Set());
  let activeWarnings = $derived(warnings.filter(w => !dismissedCodes.has(w.code)));

  function settingsLink(code) {
    return '#/settings?tab=models';
  }

  function dismissWarning(code) {
    dismissedCodes = new Set([...dismissedCodes, code]);
  }

  onMount(async () => {
    try {
      warnings = await fetchWarnings();
    } catch (e) {
      console.error('Failed to load warnings:', e);
    }
  });

  // Track which conversation we've already loaded to avoid re-fetching
  // when we ourselves trigger the activeConversationId change (e.g. during
  // streaming when conversation_meta arrives).
  let loadedId = null;

  // React to external activeConversationId changes (sidebar clicks, new chat).
  $effect(() => {
    const id = activeConversationId;
    if (id === loadedId) return;
    loadedId = id;
    if (id === null) {
      conversationId = null;
      conversationSource = null;
      displayItems = [];
      inputText = '';
    } else {
      loadConversation(id);
    }
  });

  // Auto-scroll when display items change.
  $effect(() => {
    if (messagesContainer && displayItems.length > 0) {
      requestAnimationFrame(() => {
        messagesContainer.scrollTop = messagesContainer.scrollHeight;
      });
    }
  });

  async function loadConversation(id) {
    try {
      const conv = await fetchConversation(id);
      conversationId = id;
      conversationSource = conv.source || 'web';
      displayItems = toDisplayItems(conv.messages);
    } catch (e) {
      console.error('Failed to load conversation:', e);
    }
  }

  async function sendMessage() {
    const text = inputText.trim();
    if (!text || isStreaming) return;

    const userItem = {
      kind: 'text',
      role: 'user',
      content: text,
      timestamp: new Date().toISOString(),
    };
    displayItems.push(userItem);
    inputText = '';
    isStreaming = true;

    // Only send the new user message; backend loads existing history from DB.
    const requestBody = {
      conversation_id: conversationId,
      messages: [
        {
          role: 'user',
          content: { type: 'text', text },
          timestamp: userItem.timestamp,
        },
      ],
    };

    // Assistant placeholder.
    displayItems.push({
      kind: 'text',
      role: 'assistant',
      content: '',
      timestamp: new Date().toISOString(),
    });
    let currentAssistantIdx = displayItems.length - 1;

    try {
      const response = await authFetch('/api/chat', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(requestBody),
      });

      if (!response.ok) {
        const error = await response.json();
        displayItems[currentAssistantIdx].content =
          `Error: ${error.message || 'Request failed'}`;
        isStreaming = false;
        return;
      }

      const reader = response.body.getReader();
      const decoder = new TextDecoder();
      let buffer = '';

      while (true) {
        const { done, value } = await reader.read();
        if (done) break;

        buffer += decoder.decode(value, { stream: true });
        const lines = buffer.split('\n');
        buffer = lines.pop() || '';

        for (const line of lines) {
          if (line.startsWith('data: ')) {
            const data = line.slice(6);
            try {
              const event = JSON.parse(data);

              if (event.type === 'conversation_meta') {
                conversationId = event.conversation_id;
                loadedId = event.conversation_id;
                onConversationCreated(event.conversation_id);
              } else if (event.type === 'token_delta') {
                displayItems[currentAssistantIdx].content += event.content;
              } else if (event.type === 'tool_call_start') {
                // Remove empty assistant placeholder before the tool block.
                if (
                  displayItems[currentAssistantIdx]?.kind === 'text' &&
                  !displayItems[currentAssistantIdx]?.content
                ) {
                  displayItems.splice(currentAssistantIdx, 1);
                }
                displayItems.push({
                  kind: 'tool_call',
                  id: event.id,
                  name: event.name,
                  arguments: event.arguments,
                  result: null,
                });
              } else if (event.type === 'tool_call_result') {
                const toolIdx = displayItems.findIndex(
                  (item) => item.kind === 'tool_call' && item.id === event.id,
                );
                if (toolIdx >= 0) {
                  displayItems[toolIdx].result = event.content;
                }
                // New assistant placeholder for text that may follow.
                displayItems.push({
                  kind: 'text',
                  role: 'assistant',
                  content: '',
                  timestamp: new Date().toISOString(),
                });
                currentAssistantIdx = displayItems.length - 1;
              } else if (event.type === 'warnings') {
                warnings = event.warnings;
              } else if (event.type === 'error') {
                displayItems[currentAssistantIdx].content +=
                  `\n\nError: ${event.message}`;
              } else if (event.type === 'done') {
                // Remove trailing empty assistant placeholder.
                const last = displayItems[displayItems.length - 1];
                if (last && last.kind === 'text' && !last.content) {
                  displayItems.pop();
                }
                onReloadConversations();
              }
            } catch (e) {
              console.error('Failed to parse SSE event:', e, data);
            }
          }
        }
      }
    } catch (error) {
      console.error('Chat error:', error);
      if (currentAssistantIdx < displayItems.length) {
        displayItems[currentAssistantIdx].content = `Error: ${error.message}`;
      }
    } finally {
      isStreaming = false;
    }
  }

  function handleKeydown(event) {
    if (event.key === 'Enter' && !event.shiftKey) {
      event.preventDefault();
      sendMessage();
    }
  }

  function renderMarkdown(content) {
    const rawHtml = marked.parse(content);
    return DOMPurify.sanitize(rawHtml, {
      ALLOWED_TAGS: [
        'p', 'br', 'strong', 'em', 'code', 'pre', 'a', 'ul', 'ol', 'li',
        'h1', 'h2', 'h3', 'h4', 'h5', 'h6', 'blockquote', 'table',
        'thead', 'tbody', 'tr', 'th', 'td', 'hr', 'del', 'sup', 'sub'
      ],
      ALLOWED_ATTR: ['href', 'class'],
      ALLOW_DATA_ATTR: false
    });
  }
</script>

<div class="flex-1 flex flex-col min-w-0 min-h-0">
  <!-- Header -->
  <header
    class="flex-shrink-0 flex items-center gap-3 border-b border-gray-200 dark:border-gray-800 px-4 py-3"
  >
    <h1 class="text-xl font-semibold text-gray-900 dark:text-gray-100">
      buddy
    </h1>
  </header>

  <!-- Warning banners -->
  {#if activeWarnings.length > 0}
    <div class="flex-shrink-0 px-4 pt-3 space-y-2">
      {#each activeWarnings as warning (warning.code)}
        <div
          class="flex items-center gap-2 px-3 py-2 rounded-lg text-sm
            {warning.severity === 'warning'
              ? 'bg-amber-50 dark:bg-amber-900/20 border border-amber-200 dark:border-amber-800 text-amber-800 dark:text-amber-300'
              : 'bg-blue-50 dark:bg-blue-900/20 border border-blue-200 dark:border-blue-800 text-blue-800 dark:text-blue-300'}"
        >
          {#if warning.severity === 'warning'}
            <svg class="w-4 h-4 flex-shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 9v2m0 4h.01M10.29 3.86L1.82 18a2 2 0 001.71 3h16.94a2 2 0 001.71-3L13.71 3.86a2 2 0 00-3.42 0z" />
            </svg>
          {:else}
            <svg class="w-4 h-4 flex-shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M13 16h-1v-4h-1m1-4h.01M12 2a10 10 0 100 20 10 10 0 000-20z" />
            </svg>
          {/if}
          <span class="flex-1">{warning.message}</span>
          <a
            href={settingsLink(warning.code)}
            class="flex-shrink-0 font-medium underline hover:no-underline
              {warning.severity === 'warning'
                ? 'text-amber-700 dark:text-amber-400'
                : 'text-blue-700 dark:text-blue-400'}"
          >
            Settings
          </a>
          <button
            onclick={() => dismissWarning(warning.code)}
            class="flex-shrink-0 p-0.5 rounded hover:bg-black/10 dark:hover:bg-white/10 transition-colors cursor-pointer"
            aria-label="Dismiss warning"
          >
            <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
            </svg>
          </button>
        </div>
      {/each}
    </div>
  {/if}

  <!-- Messages -->
  <div
    bind:this={messagesContainer}
    class="flex-1 overflow-y-auto px-4 py-6 space-y-4"
  >
    {#if conversationSource && conversationSource !== 'web'}
      <div class="flex justify-center mb-4">
        <span class="text-xs text-gray-400 dark:text-gray-500 bg-gray-50 dark:bg-gray-800/50 px-3 py-1 rounded-full">
          This conversation started on {conversationSource === 'telegram' ? 'Telegram' : conversationSource === 'whatsapp' ? 'WhatsApp' : conversationSource}
        </span>
      </div>
    {/if}

    {#if displayItems.length === 0 && !isStreaming}
      <div
        class="flex items-center justify-center h-full text-gray-400 dark:text-gray-500"
      >
        <p class="text-lg">Start a new conversation</p>
      </div>
    {/if}

    {#each displayItems as item, idx (idx)}
      {#if item.kind === 'text' && item.content}
        <div
          class="flex {item.role === 'user' ? 'justify-end' : 'justify-start'}"
        >
          <div
            class="max-w-[80%] rounded-lg px-4 py-3 {item.role === 'user'
              ? 'bg-blue-600 text-white'
              : 'bg-gray-100 dark:bg-gray-800 text-gray-900 dark:text-gray-100'}"
          >
            {#if item.role === 'user'}
              <p class="whitespace-pre-wrap break-words">{item.content}</p>
            {:else}
              <div
                class="prose prose-sm dark:prose-invert max-w-none prose-pre:bg-gray-900 prose-pre:text-gray-100"
              >
                {@html renderMarkdown(item.content)}
              </div>
            {/if}
          </div>
        </div>
      {:else if item.kind === 'tool_call'}
        <div class="max-w-[80%]">
          <ToolCallBlock
            name={item.name}
            arguments={item.arguments}
            result={item.result}
          />
        </div>
      {/if}
    {/each}

    {#if isStreaming}
      {#if displayItems.length === 0 || (displayItems[displayItems.length - 1]?.kind === 'text' && !displayItems[displayItems.length - 1]?.content)}
        <div class="flex justify-start">
          <div class="bg-gray-100 dark:bg-gray-800 rounded-lg px-4 py-3">
            <div class="flex space-x-2">
              <div
                class="w-2 h-2 bg-gray-400 rounded-full animate-bounce"
                style="animation-delay: 0ms;"
              ></div>
              <div
                class="w-2 h-2 bg-gray-400 rounded-full animate-bounce"
                style="animation-delay: 150ms;"
              ></div>
              <div
                class="w-2 h-2 bg-gray-400 rounded-full animate-bounce"
                style="animation-delay: 300ms;"
              ></div>
            </div>
          </div>
        </div>
      {/if}
    {/if}
  </div>

  <!-- Input -->
  <div
    class="flex-shrink-0 border-t border-gray-200 dark:border-gray-800 px-4 py-4"
  >
    <form
      onsubmit={(e) => {
        e.preventDefault();
        sendMessage();
      }}
      class="flex gap-2"
    >
      <input
        type="text"
        bind:value={inputText}
        onkeydown={handleKeydown}
        disabled={isStreaming}
        placeholder="Type a message..."
        class="flex-1 px-4 py-2 border border-gray-300 dark:border-gray-700 rounded-lg
               bg-white dark:bg-gray-800 text-gray-900 dark:text-gray-100
               placeholder-gray-500 dark:placeholder-gray-400
               focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-transparent
               disabled:opacity-50 disabled:cursor-not-allowed"
      />
      <button
        type="submit"
        disabled={isStreaming || !inputText.trim()}
        class="px-6 py-2 bg-blue-600 text-white rounded-lg font-medium
               hover:bg-blue-700 focus:outline-none focus:ring-2 focus:ring-blue-500 focus:ring-offset-2
               disabled:opacity-50 disabled:cursor-not-allowed disabled:hover:bg-blue-600
               transition-colors"
      >
        Send
      </button>
    </form>
  </div>
</div>
