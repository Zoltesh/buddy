<script>
  import { marked } from 'marked';
  import { onMount } from 'svelte';
  import {
    fetchConversations,
    fetchConversation,
    deleteConversation,
    toDisplayItems,
  } from './lib/api.js';
  import Sidebar from './lib/Sidebar.svelte';
  import ToolCallBlock from './lib/ToolCallBlock.svelte';

  marked.setOptions({ breaks: true, gfm: true });

  let conversations = $state([]);
  let activeConversationId = $state(null);
  let conversationId = $state(null);
  let displayItems = $state([]);
  let inputText = $state('');
  let isStreaming = $state(false);
  let sidebarOpen = $state(false);
  let messagesContainer;

  // Auto-scroll when display items change.
  $effect(() => {
    if (messagesContainer && displayItems.length > 0) {
      requestAnimationFrame(() => {
        messagesContainer.scrollTop = messagesContainer.scrollHeight;
      });
    }
  });

  onMount(async () => {
    await loadConversations();
  });

  async function loadConversations() {
    try {
      conversations = await fetchConversations();
    } catch (e) {
      console.error('Failed to load conversations:', e);
    }
  }

  function handleNewChat() {
    activeConversationId = null;
    conversationId = null;
    displayItems = [];
    inputText = '';
    sidebarOpen = false;
  }

  async function handleSelectConversation(id) {
    if (id === activeConversationId) {
      sidebarOpen = false;
      return;
    }
    try {
      const conv = await fetchConversation(id);
      activeConversationId = id;
      conversationId = id;
      displayItems = toDisplayItems(conv.messages);
      sidebarOpen = false;
    } catch (e) {
      console.error('Failed to load conversation:', e);
    }
  }

  async function handleDeleteConversation(id) {
    try {
      await deleteConversation(id);
      conversations = conversations.filter((c) => c.id !== id);
      if (activeConversationId === id) {
        handleNewChat();
      }
    } catch (e) {
      console.error('Failed to delete conversation:', e);
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
      const response = await fetch('/api/chat', {
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
                activeConversationId = event.conversation_id;
                await loadConversations();
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
              } else if (event.type === 'error') {
                displayItems[currentAssistantIdx].content +=
                  `\n\nError: ${event.message}`;
              } else if (event.type === 'done') {
                // Remove trailing empty assistant placeholder.
                const last = displayItems[displayItems.length - 1];
                if (last && last.kind === 'text' && !last.content) {
                  displayItems.pop();
                }
                await loadConversations();
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
    return marked.parse(content);
  }
</script>

<div class="flex h-screen bg-white dark:bg-gray-900">
  <!-- Mobile sidebar overlay -->
  {#if sidebarOpen}
    <button
      class="fixed inset-0 bg-black/50 z-20 md:hidden cursor-default"
      onclick={() => (sidebarOpen = false)}
      onkeydown={(e) => e.key === 'Escape' && (sidebarOpen = false)}
      tabindex="-1"
      aria-label="Close sidebar"
    ></button>
  {/if}

  <!-- Sidebar -->
  <aside
    class="fixed md:static inset-y-0 left-0 z-30 w-64 transform transition-transform duration-200
           {sidebarOpen ? 'translate-x-0' : '-translate-x-full'} md:translate-x-0
           border-r border-gray-200 dark:border-gray-800"
  >
    <Sidebar
      {conversations}
      activeId={activeConversationId}
      onSelect={handleSelectConversation}
      onNewChat={handleNewChat}
      onDelete={handleDeleteConversation}
    />
  </aside>

  <!-- Main chat area -->
  <div class="flex-1 flex flex-col min-w-0">
    <!-- Header -->
    <header
      class="flex-shrink-0 flex items-center gap-3 border-b border-gray-200 dark:border-gray-800 px-4 py-3"
    >
      <!-- Hamburger (mobile) -->
      <button
        class="md:hidden p-1 rounded hover:bg-gray-100 dark:hover:bg-gray-800 transition-colors cursor-pointer"
        onclick={() => (sidebarOpen = !sidebarOpen)}
        aria-label="Toggle sidebar"
      >
        <svg
          class="w-6 h-6 text-gray-600 dark:text-gray-300"
          fill="none"
          stroke="currentColor"
          viewBox="0 0 24 24"
        >
          <path
            stroke-linecap="round"
            stroke-linejoin="round"
            stroke-width="2"
            d="M4 6h16M4 12h16M4 18h16"
          />
        </svg>
      </button>
      <h1 class="text-xl font-semibold text-gray-900 dark:text-gray-100">
        buddy
      </h1>
    </header>

    <!-- Messages -->
    <div
      bind:this={messagesContainer}
      class="flex-1 overflow-y-auto px-4 py-6 space-y-4"
    >
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
</div>
