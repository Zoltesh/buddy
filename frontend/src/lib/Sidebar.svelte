<script>
  import { timeAgo } from './api.js';

  let { conversations = [], activeId = null, onSelect, onNewChat, onDelete } = $props();
</script>

<div class="flex flex-col h-full bg-gray-50 dark:bg-gray-950">
  <!-- New Chat button -->
  <div class="p-3 border-b border-gray-200 dark:border-gray-800">
    <button
      onclick={onNewChat}
      class="w-full flex items-center justify-center gap-2 px-4 py-2 bg-blue-600 text-white rounded-lg font-medium
             hover:bg-blue-700 focus:outline-none focus:ring-2 focus:ring-blue-500 transition-colors text-sm cursor-pointer"
    >
      <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
        <path
          stroke-linecap="round"
          stroke-linejoin="round"
          stroke-width="2"
          d="M12 4v16m8-8H4"
        />
      </svg>
      New Chat
    </button>
  </div>

  <!-- Conversation list -->
  <div class="flex-1 overflow-y-auto">
    {#each conversations as conv (conv.id)}
      <div
        class="group flex items-center gap-2 px-3 py-3 cursor-pointer border-l-2 transition-colors
               {conv.id === activeId
          ? 'bg-blue-50 dark:bg-blue-900/20 border-l-blue-500'
          : 'hover:bg-gray-100 dark:hover:bg-gray-900 border-l-transparent'}"
        onclick={() => onSelect(conv.id)}
        role="button"
        tabindex="0"
        onkeydown={(e) => e.key === 'Enter' && onSelect(conv.id)}
      >
        <div class="flex-1 min-w-0">
          <p class="text-sm font-medium text-gray-900 dark:text-gray-100 truncate">
            {conv.title}
          </p>
          <p class="text-xs text-gray-500 dark:text-gray-400 mt-0.5">
            {timeAgo(conv.updated_at)}
          </p>
        </div>
        <button
          class="flex-shrink-0 p-1 rounded opacity-0 group-hover:opacity-100
                 hover:bg-red-100 dark:hover:bg-red-900/30
                 text-gray-400 hover:text-red-500 transition-all cursor-pointer"
          onclick={(e) => {
            e.stopPropagation();
            onDelete(conv.id);
          }}
          title="Delete conversation"
        >
          <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path
              stroke-linecap="round"
              stroke-linejoin="round"
              stroke-width="2"
              d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16"
            />
          </svg>
        </button>
      </div>
    {/each}

    {#if conversations.length === 0}
      <div class="px-4 py-8 text-center text-sm text-gray-400 dark:text-gray-500">
        No conversations yet
      </div>
    {/if}
  </div>
</div>
