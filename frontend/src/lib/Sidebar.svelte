<script>
  import { timeAgo } from './api.js';

  let {
    conversations = [],
    activeId = null,
    currentRoute = '/',
    collapsed = false,
    authRequired = false,
    onSelect,
    onNewChat,
    onDelete,
    onToggleCollapse,
    onSignOut,
  } = $props();
</script>

<div class="flex flex-col h-full bg-gray-50 dark:bg-gray-950">
  <!-- Brand mark -->
  <div
    class="px-4 py-3 border-b border-gray-200 dark:border-gray-800 flex items-center
           {collapsed ? 'md:justify-center md:px-2' : ''}"
  >
    <span
      class="text-lg font-bold text-blue-600 dark:text-blue-400
             {collapsed ? 'md:hidden' : ''}"
    >buddy</span>
    <span
      class="text-lg font-bold text-blue-600 dark:text-blue-400
             hidden {collapsed ? 'md:inline' : ''}"
    >b</span>
  </div>

  <!-- Navigation rail -->
  <nav class="py-2 border-b border-gray-200 dark:border-gray-800">
    <a
      href="#/"
      class="flex items-center gap-3 px-4 py-2.5 text-sm transition-colors border-l-2
             focus:outline-none focus:ring-2 focus:ring-blue-500 focus:ring-inset
             {currentRoute === '/'
        ? 'bg-blue-50 dark:bg-blue-900/20 text-blue-700 dark:text-blue-300 border-l-blue-500'
        : 'text-gray-600 dark:text-gray-400 hover:bg-gray-100 dark:hover:bg-gray-900 border-l-transparent'}
             {collapsed ? 'md:justify-center md:px-2' : ''}"
      title={collapsed ? 'Chat' : undefined}
    >
      <svg class="w-5 h-5 flex-shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
        <path
          stroke-linecap="round"
          stroke-linejoin="round"
          stroke-width="2"
          d="M8 12h.01M12 12h.01M16 12h.01M21 12c0 4.418-4.03 8-9 8a9.863 9.863 0 01-4.255-.949L3 20l1.395-3.72C3.512 15.042 3 13.574 3 12c0-4.418 4.03-8 9-8s9 3.582 9 8z"
        />
      </svg>
      <span class="{collapsed ? 'md:hidden' : ''}">Chat</span>
    </a>

    <a
      href="#/settings"
      class="flex items-center gap-3 px-4 py-2.5 text-sm transition-colors border-l-2
             focus:outline-none focus:ring-2 focus:ring-blue-500 focus:ring-inset
             {currentRoute === '/settings'
        ? 'bg-blue-50 dark:bg-blue-900/20 text-blue-700 dark:text-blue-300 border-l-blue-500'
        : 'text-gray-600 dark:text-gray-400 hover:bg-gray-100 dark:hover:bg-gray-900 border-l-transparent'}
             {collapsed ? 'md:justify-center md:px-2' : ''}"
      title={collapsed ? 'Settings' : undefined}
    >
      <svg class="w-5 h-5 flex-shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
        <path
          stroke-linecap="round"
          stroke-linejoin="round"
          stroke-width="2"
          d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.066 2.573c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.573 1.066c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.066-2.573c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z"
        />
        <path
          stroke-linecap="round"
          stroke-linejoin="round"
          stroke-width="2"
          d="M15 12a3 3 0 11-6 0 3 3 0 016 0z"
        />
      </svg>
      <span class="{collapsed ? 'md:hidden' : ''}">Settings</span>
    </a>
  </nav>

  <!-- Flexible content area -->
  <div class="flex-1 flex flex-col min-h-0">
    {#if currentRoute === '/'}
      <!-- Context panel: conversation list (hidden on desktop when collapsed) -->
      <div class="flex-1 flex flex-col min-h-0 {collapsed ? 'md:hidden' : ''}">
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
    {/if}
  </div>

  <!-- Bottom actions -->
  <div class="border-t border-gray-200 dark:border-gray-800 p-2 space-y-1">
    {#if authRequired}
      <button
        onclick={onSignOut}
        class="w-full flex items-center gap-3 px-3 py-2 rounded-lg text-sm text-gray-600 dark:text-gray-400
               hover:bg-gray-100 dark:hover:bg-gray-900 focus:outline-none focus:ring-2 focus:ring-blue-500
               transition-colors cursor-pointer
               {collapsed ? 'justify-center' : ''}"
        title={collapsed ? 'Sign Out' : undefined}
        aria-label="Sign Out"
      >
        <svg class="w-4 h-4 flex-shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path
            stroke-linecap="round"
            stroke-linejoin="round"
            stroke-width="2"
            d="M17 16l4-4m0 0l-4-4m4 4H7m6 4v1a3 3 0 01-3 3H6a3 3 0 01-3-3V7a3 3 0 013-3h4a3 3 0 013 3v1"
          />
        </svg>
        <span class="{collapsed ? 'md:hidden' : ''}">Sign Out</span>
      </button>
    {/if}

    <!-- Collapse toggle (desktop only) -->
    <div class="hidden md:block">
      <button
        onclick={onToggleCollapse}
        class="w-full flex items-center justify-center p-2 rounded-lg text-gray-500 dark:text-gray-400
               hover:bg-gray-100 dark:hover:bg-gray-900 focus:outline-none focus:ring-2 focus:ring-blue-500
               transition-colors cursor-pointer"
        title={collapsed ? 'Expand sidebar' : 'Collapse sidebar'}
        aria-label={collapsed ? 'Expand sidebar' : 'Collapse sidebar'}
      >
        <svg
          class="w-4 h-4 transition-transform duration-200 {collapsed ? 'rotate-180' : ''}"
          fill="none"
          stroke="currentColor"
          viewBox="0 0 24 24"
        >
          <path
            stroke-linecap="round"
            stroke-linejoin="round"
            stroke-width="2"
            d="M11 19l-7-7 7-7m8 14l-7-7 7-7"
          />
        </svg>
      </button>
    </div>
  </div>
</div>
