<script>
  import { onMount } from 'svelte';
  import {
    fetchConversations,
    deleteConversation,
  } from './lib/api.js';
  import Sidebar from './lib/Sidebar.svelte';
  import Chat from './lib/Chat.svelte';
  import Settings from './lib/Settings.svelte';

  let conversations = $state([]);
  let activeConversationId = $state(null);
  let sidebarOpen = $state(false);
  let sidebarCollapsed = $state(localStorage.getItem('buddy-sidebar-collapsed') === 'true');
  let currentRoute = $state(getRoute());
  let hashParams = $state(getHashParams());

  function getRoute() {
    const hash = window.location.hash.slice(1) || '/';
    const qIdx = hash.indexOf('?');
    return qIdx >= 0 ? hash.slice(0, qIdx) : hash;
  }

  function getHashParams() {
    const hash = window.location.hash.slice(1) || '/';
    const qIdx = hash.indexOf('?');
    return qIdx >= 0 ? new URLSearchParams(hash.slice(qIdx + 1)) : new URLSearchParams();
  }

  onMount(() => {
    loadConversations();

    function onHashChange() {
      currentRoute = getRoute();
      hashParams = getHashParams();
      sidebarOpen = false;
    }
    window.addEventListener('hashchange', onHashChange);
    return () => window.removeEventListener('hashchange', onHashChange);
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
    sidebarOpen = false;
    if (currentRoute !== '/') {
      window.location.hash = '#/';
    }
  }

  function handleSelectConversation(id) {
    if (id === activeConversationId) {
      sidebarOpen = false;
      return;
    }
    activeConversationId = id;
    sidebarOpen = false;
    if (currentRoute !== '/') {
      window.location.hash = '#/';
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

  function handleConversationCreated(id) {
    activeConversationId = id;
    loadConversations();
  }

  function handleToggleCollapse() {
    sidebarCollapsed = !sidebarCollapsed;
    localStorage.setItem('buddy-sidebar-collapsed', String(sidebarCollapsed));
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
    class="fixed md:static inset-y-0 left-0 z-30 w-64 {sidebarCollapsed ? 'md:w-14' : ''} overflow-hidden
           transform transition-all duration-200
           {sidebarOpen ? 'translate-x-0' : '-translate-x-full'} md:translate-x-0
           border-r border-gray-200 dark:border-gray-800"
  >
    <Sidebar
      {conversations}
      activeId={activeConversationId}
      {currentRoute}
      collapsed={sidebarCollapsed}
      onSelect={handleSelectConversation}
      onNewChat={handleNewChat}
      onDelete={handleDeleteConversation}
      onToggleCollapse={handleToggleCollapse}
    />
  </aside>

  <!-- Mobile hamburger (floats above routed content) -->
  <button
    class="fixed top-3 left-3 z-10 md:hidden p-1 rounded hover:bg-gray-100 dark:hover:bg-gray-800 transition-colors cursor-pointer"
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

  <!-- Routed content -->
  <div class="flex-1 flex flex-col min-w-0" class:hidden={currentRoute === '/settings'}>
    <Chat
      {activeConversationId}
      onConversationCreated={handleConversationCreated}
      onReloadConversations={loadConversations}
    />
  </div>

  {#if currentRoute === '/settings'}
    <Settings initialTab={hashParams.get('tab')} />
  {/if}
</div>
