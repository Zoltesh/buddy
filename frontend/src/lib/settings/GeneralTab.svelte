<script>
  import { onMount } from 'svelte';
  import { formatApiError, putConfigChat, putConfigMemory } from '../api.js';

  let { config = $bindable() } = $props();

  let systemPrompt = $state('');
  let autoRetrieve = $state(true);
  let similarityThreshold = $state(0.5);
  let autoRetrieveLimit = $state(3);
  let saving = $state(false);
  let saveMessage = $state(null);

  function syncFormState() {
    systemPrompt = config.chat?.system_prompt ?? '';
    autoRetrieve = config.memory?.auto_retrieve ?? true;
    similarityThreshold = config.memory?.similarity_threshold ?? 0.5;
    autoRetrieveLimit = config.memory?.auto_retrieve_limit ?? 3;
  }

  onMount(() => syncFormState());

  async function saveGeneral() {
    saving = true;
    saveMessage = null;
    try {
      let updated = await putConfigChat({ system_prompt: systemPrompt });
      updated = await putConfigMemory({
        auto_retrieve: autoRetrieve,
        similarity_threshold: similarityThreshold,
        auto_retrieve_limit: autoRetrieveLimit,
      });
      config = updated;
      syncFormState();
      saveMessage = { type: 'success', text: 'Settings saved.' };
    } catch (e) {
      saveMessage = { type: 'error', text: formatApiError(e) };
    } finally {
      saving = false;
    }
  }
</script>

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
