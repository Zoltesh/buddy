<script>
  import { onMount } from 'svelte';
  import { fetchConfig } from './api.js';
  import GeneralTab from './settings/GeneralTab.svelte';
  import ModelsTab from './settings/ModelsTab.svelte';
  import SkillsTab from './settings/SkillsTab.svelte';

  let { initialTab = null } = $props();

  let config = $state(null);
  let loading = $state(true);
  let error = $state(null);
  let activeTab = $state('general');

  const tabs = [
    { id: 'general', label: 'General' },
    { id: 'models', label: 'Models' },
    { id: 'skills', label: 'Skills' },
  ];

  onMount(() => {
    if (tabs.some(t => t.id === initialTab)) {
      activeTab = initialTab;
    }
    loadConfig();
  });

  async function loadConfig() {
    loading = true;
    error = null;
    try {
      config = await fetchConfig();
    } catch (e) {
      error = e.message;
    } finally {
      loading = false;
    }
  }
</script>

<div class="flex-1 flex flex-col min-w-0">
  <!-- Header -->
  <header
    class="flex-shrink-0 flex items-center gap-3 border-b border-gray-200 dark:border-gray-800 px-4 py-3"
  >
    <a
      href="#/"
      class="p-1 rounded hover:bg-gray-100 dark:hover:bg-gray-800 transition-colors"
      aria-label="Back to chat"
    >
      <svg
        class="w-5 h-5 text-gray-600 dark:text-gray-300"
        fill="none"
        stroke="currentColor"
        viewBox="0 0 24 24"
      >
        <path
          stroke-linecap="round"
          stroke-linejoin="round"
          stroke-width="2"
          d="M15 19l-7-7 7-7"
        />
      </svg>
    </a>
    <h1 class="text-xl font-semibold text-gray-900 dark:text-gray-100">
      Settings
    </h1>
  </header>

  <!-- Content -->
  <div class="flex-1 overflow-y-auto px-6 py-6">
    <div class="max-w-2xl mx-auto">
      {#if loading}
        <div class="flex items-center gap-2 text-gray-500 dark:text-gray-400 py-8">
          <svg class="w-5 h-5 animate-spin" viewBox="0 0 24 24" fill="none">
            <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
            <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z"></path>
          </svg>
          Loading settings...
        </div>
      {:else if error}
        <div class="rounded-lg border border-red-200 dark:border-red-800 bg-red-50 dark:bg-red-900/20 px-4 py-3 text-red-700 dark:text-red-400">
          <p class="font-medium">Failed to load settings</p>
          <p class="text-sm mt-1">{error}</p>
          <button
            onclick={loadConfig}
            class="mt-2 text-sm underline hover:no-underline"
          >
            Try again
          </button>
        </div>
      {:else}
        <!-- Tabs -->
        <div class="flex gap-1 border-b border-gray-200 dark:border-gray-800 mb-6">
          {#each tabs as tab}
            <button
              onclick={() => (activeTab = tab.id)}
              class="px-4 py-2 text-sm font-medium transition-colors cursor-pointer
                {activeTab === tab.id
                  ? 'text-blue-600 dark:text-blue-400 border-b-2 border-blue-600 dark:border-blue-400 -mb-px'
                  : 'text-gray-500 dark:text-gray-400 hover:text-gray-700 dark:hover:text-gray-300'}"
            >
              {tab.label}
            </button>
          {/each}
        </div>

        {#if activeTab === 'general'}
          <GeneralTab bind:config />
        {:else if activeTab === 'models'}
          <ModelsTab bind:config />
        {:else if activeTab === 'skills'}
          <SkillsTab bind:config />
        {/if}
      {/if}
    </div>
  </div>
</div>
