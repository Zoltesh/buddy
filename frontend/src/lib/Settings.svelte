<script>
  import { onMount } from 'svelte';
  import { fetchConfig, putConfigChat, putConfigMemory } from './api.js';

  let config = $state(null);
  let loading = $state(true);
  let error = $state(null);
  let activeTab = $state('general');

  // General section form state
  let systemPrompt = $state('');
  let autoRetrieve = $state(true);
  let similarityThreshold = $state(0.5);
  let autoRetrieveLimit = $state(3);
  let saving = $state(false);
  let saveMessage = $state(null);

  onMount(loadConfig);

  async function loadConfig() {
    loading = true;
    error = null;
    try {
      config = await fetchConfig();
      syncFormState();
    } catch (e) {
      error = e.message;
    } finally {
      loading = false;
    }
  }

  function syncFormState() {
    systemPrompt = config.chat?.system_prompt ?? '';
    autoRetrieve = config.memory?.auto_retrieve ?? true;
    similarityThreshold = config.memory?.similarity_threshold ?? 0.5;
    autoRetrieveLimit = config.memory?.auto_retrieve_limit ?? 3;
  }

  async function saveGeneral() {
    saving = true;
    saveMessage = null;
    try {
      await putConfigChat({ system_prompt: systemPrompt });
      const updated = await putConfigMemory({
        auto_retrieve: autoRetrieve,
        auto_retrieve_limit: autoRetrieveLimit,
        similarity_threshold: similarityThreshold,
      });
      config = updated;
      syncFormState();
      saveMessage = { type: 'success', text: 'Settings saved.' };
    } catch (e) {
      const details = e.details;
      if (details?.errors) {
        saveMessage = {
          type: 'error',
          text: details.errors.map((err) => `${err.field}: ${err.message}`).join('; '),
        };
      } else {
        saveMessage = { type: 'error', text: details?.message || e.message };
      }
    } finally {
      saving = false;
    }
  }

  const tabs = [
    { id: 'general', label: 'General' },
    { id: 'models', label: 'Models' },
    { id: 'skills', label: 'Skills' },
  ];

  const skillDefs = [
    { key: 'read_file', label: 'Read File' },
    { key: 'write_file', label: 'Write File' },
    { key: 'fetch_url', label: 'Fetch URL' },
  ];
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

        <!-- General Tab -->
        {#if activeTab === 'general'}
          <div class="space-y-6">
            <!-- Server Info (read-only) -->
            <section>
              <h2 class="text-sm font-semibold text-gray-900 dark:text-gray-100 uppercase tracking-wide mb-3">Server</h2>
              <div class="rounded-lg border border-gray-200 dark:border-gray-800 bg-gray-50 dark:bg-gray-800/50 px-4 py-3">
                <div class="flex gap-6 text-sm">
                  <div>
                    <span class="text-gray-500 dark:text-gray-400">Host:</span>
                    <span class="ml-1 text-gray-900 dark:text-gray-100">{config.server.host}</span>
                  </div>
                  <div>
                    <span class="text-gray-500 dark:text-gray-400">Port:</span>
                    <span class="ml-1 text-gray-900 dark:text-gray-100">{config.server.port}</span>
                  </div>
                </div>
              </div>
            </section>

            <!-- System Prompt -->
            <section>
              <h2 class="text-sm font-semibold text-gray-900 dark:text-gray-100 uppercase tracking-wide mb-3">Chat</h2>
              <label class="block text-sm text-gray-700 dark:text-gray-300 mb-1" for="system-prompt">
                System Prompt
              </label>
              <textarea
                id="system-prompt"
                bind:value={systemPrompt}
                rows="4"
                class="w-full px-3 py-2 border border-gray-300 dark:border-gray-700 rounded-lg
                       bg-white dark:bg-gray-800 text-gray-900 dark:text-gray-100
                       placeholder-gray-500 dark:placeholder-gray-400
                       focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-transparent
                       text-sm resize-y"
              ></textarea>
            </section>

            <!-- Memory Settings -->
            <section>
              <h2 class="text-sm font-semibold text-gray-900 dark:text-gray-100 uppercase tracking-wide mb-3">Memory</h2>
              <div class="space-y-4">
                <label class="flex items-center gap-3 cursor-pointer">
                  <input
                    type="checkbox"
                    bind:checked={autoRetrieve}
                    class="w-4 h-4 rounded border-gray-300 dark:border-gray-600 text-blue-600 focus:ring-blue-500"
                  />
                  <span class="text-sm text-gray-700 dark:text-gray-300">Auto-retrieve memories</span>
                </label>

                <div>
                  <label class="block text-sm text-gray-700 dark:text-gray-300 mb-1" for="similarity-threshold">
                    Similarity threshold
                  </label>
                  <input
                    id="similarity-threshold"
                    type="number"
                    bind:value={similarityThreshold}
                    min="0"
                    max="1"
                    step="0.05"
                    class="w-32 px-3 py-2 border border-gray-300 dark:border-gray-700 rounded-lg
                           bg-white dark:bg-gray-800 text-gray-900 dark:text-gray-100
                           focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-transparent
                           text-sm"
                  />
                </div>

                <div>
                  <label class="block text-sm text-gray-700 dark:text-gray-300 mb-1" for="auto-retrieve-limit">
                    Auto-retrieve limit
                  </label>
                  <input
                    id="auto-retrieve-limit"
                    type="number"
                    bind:value={autoRetrieveLimit}
                    min="1"
                    max="50"
                    step="1"
                    class="w-32 px-3 py-2 border border-gray-300 dark:border-gray-700 rounded-lg
                           bg-white dark:bg-gray-800 text-gray-900 dark:text-gray-100
                           focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-transparent
                           text-sm"
                  />
                </div>
              </div>
            </section>

            <!-- Save -->
            <div class="flex items-center gap-3 pt-2">
              <button
                onclick={saveGeneral}
                disabled={saving}
                class="px-6 py-2 bg-blue-600 text-white rounded-lg font-medium hover:bg-blue-700
                       focus:outline-none focus:ring-2 focus:ring-blue-500 focus:ring-offset-2
                       disabled:opacity-50 disabled:cursor-not-allowed transition-colors cursor-pointer"
              >
                {saving ? 'Saving...' : 'Save'}
              </button>
              {#if saveMessage}
                <span
                  class="text-sm {saveMessage.type === 'success'
                    ? 'text-green-600 dark:text-green-400'
                    : 'text-red-600 dark:text-red-400'}"
                >
                  {saveMessage.text}
                </span>
              {/if}
            </div>
          </div>

        <!-- Models Tab -->
        {:else if activeTab === 'models'}
          <div class="space-y-6">
            <!-- Chat Providers -->
            <section>
              <h2 class="text-sm font-semibold text-gray-900 dark:text-gray-100 uppercase tracking-wide mb-3">Chat Providers</h2>
              <div class="space-y-2">
                {#each config.models.chat.providers as provider, i}
                  <div class="rounded-lg border border-gray-200 dark:border-gray-800 bg-gray-50 dark:bg-gray-800/50 px-4 py-3">
                    <div class="flex items-center gap-2 mb-1">
                      <span class="text-xs font-medium uppercase text-gray-400 dark:text-gray-500">#{i + 1}</span>
                      <span class="text-sm font-medium text-gray-900 dark:text-gray-100">{provider.type}</span>
                    </div>
                    <div class="text-sm text-gray-600 dark:text-gray-400 space-y-0.5">
                      <div>Model: <span class="text-gray-900 dark:text-gray-200">{provider.model}</span></div>
                      {#if provider.endpoint}
                        <div>Endpoint: <span class="text-gray-900 dark:text-gray-200">{provider.endpoint}</span></div>
                      {/if}
                      {#if provider.api_key_env}
                        <div>API Key: <span class="text-gray-900 dark:text-gray-200">${provider.api_key_env}</span></div>
                      {/if}
                    </div>
                  </div>
                {/each}
              </div>
            </section>

            <!-- Embedding Providers -->
            <section>
              <h2 class="text-sm font-semibold text-gray-900 dark:text-gray-100 uppercase tracking-wide mb-3">Embedding Providers</h2>
              {#if config.models.embedding && config.models.embedding.providers.length > 0}
                <div class="space-y-2">
                  {#each config.models.embedding.providers as provider, i}
                    <div class="rounded-lg border border-gray-200 dark:border-gray-800 bg-gray-50 dark:bg-gray-800/50 px-4 py-3">
                      <div class="flex items-center gap-2 mb-1">
                        <span class="text-xs font-medium uppercase text-gray-400 dark:text-gray-500">#{i + 1}</span>
                        <span class="text-sm font-medium text-gray-900 dark:text-gray-100">{provider.type}</span>
                      </div>
                      <div class="text-sm text-gray-600 dark:text-gray-400 space-y-0.5">
                        <div>Model: <span class="text-gray-900 dark:text-gray-200">{provider.model}</span></div>
                        {#if provider.endpoint}
                          <div>Endpoint: <span class="text-gray-900 dark:text-gray-200">{provider.endpoint}</span></div>
                        {/if}
                      </div>
                    </div>
                  {/each}
                </div>
              {:else}
                <p class="text-sm text-gray-500 dark:text-gray-400 italic">Not configured</p>
              {/if}
            </section>
          </div>

        <!-- Skills Tab -->
        {:else if activeTab === 'skills'}
          <div class="space-y-6">
            {#each skillDefs as skill}
              <section>
                <h2 class="text-sm font-semibold text-gray-900 dark:text-gray-100 uppercase tracking-wide mb-3">{skill.label}</h2>
                {#if config.skills[skill.key]}
                  <div class="rounded-lg border border-gray-200 dark:border-gray-800 bg-gray-50 dark:bg-gray-800/50 px-4 py-3 text-sm">
                    {#if config.skills[skill.key].allowed_directories}
                      <div class="text-gray-600 dark:text-gray-400 mb-1">Allowed directories:</div>
                      <ul class="list-disc list-inside text-gray-900 dark:text-gray-200">
                        {#each config.skills[skill.key].allowed_directories as dir}
                          <li>{dir}</li>
                        {/each}
                      </ul>
                    {/if}
                    {#if config.skills[skill.key].allowed_domains}
                      <div class="text-gray-600 dark:text-gray-400 mb-1">Allowed domains:</div>
                      <ul class="list-disc list-inside text-gray-900 dark:text-gray-200">
                        {#each config.skills[skill.key].allowed_domains as domain}
                          <li>{domain}</li>
                        {/each}
                      </ul>
                    {/if}
                    {#if config.skills[skill.key].approval}
                      <div class="mt-2 text-gray-600 dark:text-gray-400">
                        Approval: <span class="text-gray-900 dark:text-gray-200">{config.skills[skill.key].approval}</span>
                      </div>
                    {/if}
                  </div>
                {:else}
                  <p class="text-sm text-gray-500 dark:text-gray-400 italic">Not configured</p>
                {/if}
              </section>
            {/each}
          </div>
        {/if}
      {/if}
    </div>
  </div>
</div>
