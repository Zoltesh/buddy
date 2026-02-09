<script>
  import { onDestroy } from 'svelte';
  import { discoverModels, formatApiError, putConfigModels, testProvider } from '../api.js';

  let { config = $bindable() } = $props();

  // editingProvider: { slot: 'chat'|'embedding', index: number|null, form: {...} }
  // index === null means adding new; number means editing existing
  let editingProvider = $state(null);
  let modelErrors = $state({});
  let testing = $state(false);
  let testResult = $state(null);
  let savingModels = $state(false);
  let modelsSaveMessage = $state(null);
  let confirmingDelete = $state(null); // { slot, index }

  // Provider health-check state
  let providerStatus = $state({}); // keyed by "chat_0", "embedding_1", etc.
  let recheckRunning = $state(false);
  let touchedFields = $state({}); // tracks which form fields have been blurred

  // Model discovery state
  let discovering = $state(false);
  let discoveredModels = $state(null); // array or null
  let discoveryError = $state(null);

  // Drag-and-drop reorder state
  let drag = $state(null); // { slot, fromIndex, currentIndex, snapshot }

  // ── Provider testing ────────────────────────────────────────────────

  async function testProviderEntry(slotKey, index, entry) {
    const key = `${slotKey}_${index}`;
    providerStatus = { ...providerStatus, [key]: { state: 'testing', message: '' } };
    try {
      const result = await testProvider(entry);
      providerStatus = { ...providerStatus, [key]: { state: result.status === 'ok' ? 'ok' : 'error', message: result.message ?? '' } };
    } catch (e) {
      const msg = formatApiError(e, { includeField: false });
      providerStatus = { ...providerStatus, [key]: { state: 'error', message: msg } };
    }
  }

  async function testAllProviders() {
    if (!config) return;
    recheckRunning = true;
    const fresh = {};
    const chatProviders = config.models.chat.providers ?? [];
    const embProviders = config.models.embedding?.providers ?? [];
    for (let i = 0; i < chatProviders.length; i++) fresh[`chat_${i}`] = { state: 'testing', message: '' };
    for (let i = 0; i < embProviders.length; i++) fresh[`embedding_${i}`] = { state: 'testing', message: '' };
    providerStatus = fresh;
    const promises = [
      ...chatProviders.map((p, i) => testProviderEntry('chat', i, p)),
      ...embProviders.map((p, i) => testProviderEntry('embedding', i, p)),
    ];
    await Promise.allSettled(promises);
    recheckRunning = false;
  }

  // ── Drag and drop ───────────────────────────────────────────────────

  function handleDragStart(slot, index, e) {
    if (e.button !== 0 || editingProvider || savingModels) return;
    const providers = slot === 'chat' ? config.models.chat.providers : (config.models.embedding?.providers ?? []);
    if (providers.length <= 1) return;
    e.preventDefault();
    drag = {
      slot,
      fromIndex: index,
      currentIndex: index,
      snapshot: providers.map(p => ({ ...p })),
    };
    document.addEventListener('pointermove', onDragPointerMove);
    document.addEventListener('pointerup', onDragPointerUp);
  }

  function onDragPointerMove(e) {
    if (!drag) return;
    const el = document.elementFromPoint(e.clientX, e.clientY);
    if (!el) return;
    const card = el.closest('[data-drag-index]');
    if (!card || card.dataset.dragSlot !== drag.slot) return;
    const overIndex = parseInt(card.dataset.dragIndex, 10);
    if (isNaN(overIndex) || overIndex === drag.currentIndex) return;
    const providers = drag.slot === 'chat' ? config.models.chat.providers : config.models.embedding.providers;
    const [item] = providers.splice(drag.currentIndex, 1);
    providers.splice(overIndex, 0, item);
    drag = { ...drag, currentIndex: overIndex };
  }

  function onDragPointerUp() {
    document.removeEventListener('pointermove', onDragPointerMove);
    document.removeEventListener('pointerup', onDragPointerUp);
    if (!drag) return;
    const { slot, fromIndex, currentIndex, snapshot } = drag;
    drag = null;
    if (fromIndex === currentIndex) return;
    persistReorder(slot, snapshot);
  }

  async function persistReorder(slot, snapshot) {
    const providers = slot === 'chat' ? config.models.chat.providers : config.models.embedding.providers;
    savingModels = true;
    modelsSaveMessage = null;
    try {
      const updated = await putConfigModels(buildModelsPayload(slot, providers));
      config = updated;
      providerStatus = {};
    } catch (e) {
      if (slot === 'chat') {
        config.models.chat.providers = snapshot;
      } else if (config.models.embedding) {
        config.models.embedding.providers = snapshot;
      }
      modelsSaveMessage = { type: 'error', text: formatApiError(e) };
    } finally {
      savingModels = false;
    }
  }

  onDestroy(() => {
    document.removeEventListener('pointermove', onDragPointerMove);
    document.removeEventListener('pointerup', onDragPointerUp);
  });

  // ── Provider form helpers ───────────────────────────────────────────

  const providerTypes = [
    { value: 'openai', label: 'OpenAI' },
    { value: 'lmstudio', label: 'LM Studio' },
    { value: 'local', label: 'Local' },
  ];

  const endpointPlaceholders = {
    openai: 'https://api.openai.com/v1',
    lmstudio: 'http://localhost:1234/v1',
  };

  function showsApiKey(type) {
    return type === 'openai';
  }

  function showsEndpoint(type) {
    return type !== 'local';
  }

  function providerLabel(type) {
    return providerTypes.find(pt => pt.value === type)?.label ?? type;
  }

  function positionLabel(index) {
    return index === 0 ? 'Primary' : `#${index + 1}`;
  }

  function startAdd(slot) {
    editingProvider = {
      slot,
      index: null,
      form: { type: 'lmstudio', model: '', endpoint: '', api_key: '', api_key_env: '' },
    };
    modelErrors = {};
    touchedFields = {};
    testResult = null;
    discoveredModels = null;
    discoveryError = null;
  }

  function startEdit(slot, index) {
    const providers = slot === 'chat' ? config.models.chat.providers : (config.models.embedding?.providers ?? []);
    const p = providers[index];
    editingProvider = {
      slot,
      index,
      form: { type: p.type, model: p.model, endpoint: p.endpoint ?? '', api_key: p.api_key ?? '', api_key_env: p.api_key_env ?? '' },
    };
    modelErrors = {};
    touchedFields = {};
    testResult = null;
    discoveredModels = null;
    discoveryError = null;
  }

  function cancelEdit() {
    editingProvider = null;
    modelErrors = {};
    touchedFields = {};
    testResult = null;
    discoveredModels = null;
    discoveryError = null;
  }

  function markTouched(field) {
    touchedFields = { ...touchedFields, [field]: true };
  }

  function getFieldError(field) {
    if (!touchedFields[field]) return null;
    const errors = validateForm(editingProvider?.form ?? {});
    return errors[field] ?? null;
  }

  function validateForm(form) {
    const errors = {};
    if (!form.model?.trim()) errors.model = 'Model is required';
    return errors;
  }

  function buildModelsPayload(slot, providers) {
    return slot === 'chat'
      ? { chat: { providers }, embedding: config.models.embedding ?? undefined }
      : { chat: config.models.chat, embedding: { providers } };
  }

  function toEntry(form) {
    const entry = { type: form.type, model: form.model };
    if (form.endpoint?.trim()) entry.endpoint = form.endpoint.trim();
    if (form.api_key?.trim()) entry.api_key = form.api_key.trim();
    if (form.api_key_env?.trim()) entry.api_key_env = form.api_key_env.trim();
    return entry;
  }

  // ── CRUD operations ─────────────────────────────────────────────────

  async function saveProvider() {
    const { slot, index, form } = editingProvider;
    const errors = validateForm(form);
    if (Object.keys(errors).length > 0) {
      modelErrors = errors;
      touchedFields = Object.fromEntries(Object.keys(errors).map(k => [k, true]));
      return;
    }
    savingModels = true;
    modelsSaveMessage = null;
    const currentProviders = slot === 'chat'
      ? [...config.models.chat.providers]
      : [...(config.models.embedding?.providers ?? [])];
    const entry = toEntry(form);
    if (index === null) {
      currentProviders.push(entry);
    } else {
      currentProviders[index] = entry;
    }
    try {
      const updated = await putConfigModels(buildModelsPayload(slot, currentProviders));
      config = updated;
      editingProvider = null;
      modelErrors = {};
      testResult = null;
      modelsSaveMessage = { type: 'success', text: 'Provider saved.' };
      providerStatus = {};
    } catch (e) {
      modelsSaveMessage = { type: 'error', text: formatApiError(e) };
    } finally {
      savingModels = false;
    }
  }

  async function deleteProvider(slot, index) {
    const currentProviders = slot === 'chat'
      ? [...config.models.chat.providers]
      : [...(config.models.embedding?.providers ?? [])];

    if (slot === 'chat' && currentProviders.length <= 1) {
      modelsSaveMessage = { type: 'error', text: 'Chat must have at least one provider.' };
      return;
    }

    currentProviders.splice(index, 1);
    savingModels = true;
    modelsSaveMessage = null;
    try {
      const updated = await putConfigModels(buildModelsPayload(slot, currentProviders));
      config = updated;
      confirmingDelete = null;
      if (editingProvider?.slot === slot && editingProvider?.index === index) {
        editingProvider = null;
      }
      modelsSaveMessage = { type: 'success', text: 'Provider removed.' };
      providerStatus = {};
    } catch (e) {
      modelsSaveMessage = { type: 'error', text: formatApiError(e) };
    } finally {
      savingModels = false;
    }
  }

  async function runTestConnection() {
    const { form } = editingProvider;
    const errors = validateForm(form);
    if (Object.keys(errors).length > 0) { modelErrors = errors; return; }
    testing = true;
    testResult = null;
    try {
      const result = await testProvider(toEntry(form));
      testResult = result;
    } catch (e) {
      testResult = { status: 'error', message: formatApiError(e, { includeField: false }) };
    } finally {
      testing = false;
    }
  }

  async function runDiscoverModels() {
    const endpoint = editingProvider?.form?.endpoint?.trim();
    if (!endpoint) {
      discoveryError = 'Enter an endpoint first';
      return;
    }
    discovering = true;
    discoveredModels = null;
    discoveryError = null;
    try {
      const result = await discoverModels(endpoint);
      if (result.status === 'ok' && result.models?.length > 0) {
        discoveredModels = result.models;
      } else if (result.status === 'ok') {
        discoveryError = 'No models found';
      } else {
        discoveryError = result.message || 'Discovery failed';
      }
    } catch (e) {
      discoveryError = formatApiError(e, { includeField: false });
    } finally {
      discovering = false;
    }
  }

  function selectDiscoveredModel(id) {
    editingProvider.form.model = id;
    discoveredModels = null;
    discoveryError = null;
  }

  // ── Section status ──────────────────────────────────────────────────

  function chatSectionStatus() {
    const providers = config?.models.chat.providers ?? [];
    if (providers.length === 0) return { level: 'error', label: 'No providers configured' };
    const statuses = providers.map((_, i) => providerStatus[`chat_${i}`]);
    if (statuses.some(s => s?.state === 'testing')) return { level: 'testing', label: 'Checking...' };
    if (statuses.some(s => s?.state === 'error')) return { level: 'warning', label: 'Some providers unreachable' };
    if (statuses.every(s => s?.state === 'ok')) return { level: 'ok', label: 'All reachable' };
    return null;
  }

  function embeddingSectionStatus() {
    const providers = config?.models.embedding?.providers ?? [];
    if (providers.length === 0) return null;
    const statuses = providers.map((_, i) => providerStatus[`embedding_${i}`]);
    if (statuses.some(s => s?.state === 'testing')) return { level: 'testing', label: 'Checking...' };
    if (statuses.some(s => s?.state === 'error')) return { level: 'warning', label: 'Some providers unreachable' };
    if (statuses.every(s => s?.state === 'ok')) return { level: 'ok', label: 'All reachable' };
    return null;
  }
</script>

{#snippet providerForm()}
  <div class="rounded-lg border border-blue-200 dark:border-blue-800 bg-blue-50/50 dark:bg-blue-900/10 px-4 py-4 space-y-3">
    <div>
      <label class="block text-sm text-gray-700 dark:text-gray-300 mb-1" for="pf-type">Provider Type</label>
      <select
        id="pf-type"
        bind:value={editingProvider.form.type}
        onchange={() => { testResult = null; }}
        class="w-full px-3 py-2 border border-gray-300 dark:border-gray-700 rounded-lg
               bg-white dark:bg-gray-800 text-gray-900 dark:text-gray-100
               focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-transparent text-sm cursor-pointer"
      >
        {#each providerTypes as pt}
          <option value={pt.value}>{pt.label}</option>
        {/each}
      </select>
    </div>

    <div>
      <label class="block text-sm text-gray-700 dark:text-gray-300 mb-1" for="pf-model">
        Model <span class="text-red-500">*</span>
      </label>
      <input
        id="pf-model"
        type="text"
        bind:value={editingProvider.form.model}
        onblur={() => markTouched('model')}
        placeholder="e.g. gpt-4o, deepseek-coder"
        class="w-full px-3 py-2 border rounded-lg bg-white dark:bg-gray-800 text-gray-900 dark:text-gray-100
               focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-transparent text-sm
               {modelErrors.model || getFieldError('model') ? 'border-red-400 dark:border-red-600' : 'border-gray-300 dark:border-gray-700'}"
      />
      {#if modelErrors.model || getFieldError('model')}
        <p class="text-xs text-red-600 dark:text-red-400 mt-1">{modelErrors.model || getFieldError('model')}</p>
      {/if}
      {#if editingProvider.form.type === 'lmstudio'}
        <div class="flex items-center gap-2 mt-1.5">
          <button
            onclick={runDiscoverModels}
            disabled={discovering || savingModels}
            class="px-2.5 py-1 text-xs border border-gray-300 dark:border-gray-600 rounded-md
                   text-gray-600 dark:text-gray-400 hover:bg-gray-100 dark:hover:bg-gray-800
                   disabled:opacity-50 disabled:cursor-not-allowed transition-colors cursor-pointer"
          >
            {discovering ? 'Discovering...' : 'Discover Models'}
          </button>
          {#if discoveryError}
            <span class="text-xs text-red-600 dark:text-red-400">{discoveryError}</span>
          {/if}
        </div>
        {#if discoveredModels}
          <div class="mt-1.5 rounded-md border border-gray-200 dark:border-gray-700 bg-white dark:bg-gray-800 max-h-40 overflow-y-auto">
            {#each discoveredModels as m}
              <button
                onclick={() => selectDiscoveredModel(m.id)}
                class="w-full text-left px-3 py-1.5 text-sm hover:bg-blue-50 dark:hover:bg-blue-900/30
                       transition-colors cursor-pointer
                       {m.id === editingProvider.form.model ? 'bg-blue-50 dark:bg-blue-900/30 font-medium' : ''}"
              >
                <span class="text-gray-900 dark:text-gray-100">{m.id}</span>
                {#if m.loaded != null}
                  <span class="ml-2 text-xs {m.loaded ? 'text-green-600 dark:text-green-400' : 'text-gray-400 dark:text-gray-500'}">
                    {m.loaded ? 'loaded' : 'not loaded'}
                  </span>
                {/if}
                {#if m.context_length}
                  <span class="ml-2 text-xs text-gray-400 dark:text-gray-500">{m.context_length.toLocaleString()} ctx</span>
                {/if}
              </button>
            {/each}
          </div>
        {/if}
      {/if}
    </div>

    {#if showsEndpoint(editingProvider.form.type)}
      <div>
        <label class="block text-sm text-gray-700 dark:text-gray-300 mb-1" for="pf-endpoint">Endpoint</label>
        <input
          id="pf-endpoint"
          type="text"
          bind:value={editingProvider.form.endpoint}
          placeholder={endpointPlaceholders[editingProvider.form.type] ?? ''}
          class="w-full px-3 py-2 border border-gray-300 dark:border-gray-700 rounded-lg
                 bg-white dark:bg-gray-800 text-gray-900 dark:text-gray-100
                 focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-transparent text-sm"
        />
      </div>
    {/if}

    {#if showsApiKey(editingProvider.form.type)}
      <div>
        <label class="block text-sm text-gray-700 dark:text-gray-300 mb-1" for="pf-apikey">API Key</label>
        <input
          id="pf-apikey"
          type="password"
          bind:value={editingProvider.form.api_key}
          placeholder="sk-..."
          autocomplete="off"
          class="w-full px-3 py-2 border border-gray-300 dark:border-gray-700 rounded-lg
                 bg-white dark:bg-gray-800 text-gray-900 dark:text-gray-100
                 focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-transparent text-sm"
        />
      </div>
    {/if}

    <!-- Test Connection -->
    <div class="flex items-center gap-3 pt-1">
      <button
        onclick={runTestConnection}
        disabled={testing || savingModels}
        class="px-3 py-1.5 text-sm border border-gray-300 dark:border-gray-600 rounded-lg
               text-gray-700 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-800
               disabled:opacity-50 disabled:cursor-not-allowed transition-colors cursor-pointer"
      >
        {testing ? 'Testing...' : 'Test Connection'}
      </button>
      {#if testResult}
        <span class="text-sm {testResult.status === 'ok' ? 'text-green-600 dark:text-green-400' : 'text-red-600 dark:text-red-400'} flex items-center gap-1">
          {#if testResult.status === 'ok'}
            <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 13l4 4L19 7" /></svg>
          {:else}
            <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" /></svg>
          {/if}
          {testResult.message}
        </span>
      {/if}
    </div>

    <!-- Save / Cancel -->
    <div class="flex items-center gap-2 pt-1 border-t border-gray-200 dark:border-gray-700">
      <button
        onclick={saveProvider}
        disabled={savingModels}
        class="mt-2 px-4 py-1.5 bg-blue-600 text-white text-sm rounded-lg font-medium hover:bg-blue-700
               focus:outline-none focus:ring-2 focus:ring-blue-500 focus:ring-offset-2
               disabled:opacity-50 disabled:cursor-not-allowed transition-colors cursor-pointer"
      >
        {savingModels ? 'Saving...' : 'Save'}
      </button>
      <button
        onclick={cancelEdit}
        disabled={savingModels}
        class="mt-2 px-4 py-1.5 text-sm text-gray-600 dark:text-gray-400 hover:text-gray-800 dark:hover:text-gray-200
               disabled:opacity-50 transition-colors cursor-pointer"
      >
        Cancel
      </button>
    </div>
  </div>
{/snippet}

{#snippet statusIcon(status)}
  {#if status?.state === 'testing'}
    <svg class="w-4 h-4 animate-spin text-gray-400" viewBox="0 0 24 24" fill="none">
      <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
      <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z"></path>
    </svg>
  {:else if status?.state === 'ok'}
    <svg class="w-4 h-4 text-green-500" fill="none" stroke="currentColor" viewBox="0 0 24 24" aria-label="Reachable"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 13l4 4L19 7" /></svg>
  {:else if status?.state === 'error'}
    <svg class="w-4 h-4 text-red-500" fill="none" stroke="currentColor" viewBox="0 0 24 24" aria-label="Unreachable"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" /></svg>
  {/if}
{/snippet}

{#snippet sectionIndicator(info)}
  {#if info}
    <span class="inline-flex items-center gap-1 text-xs font-medium
      {info.level === 'error' ? 'text-red-600 dark:text-red-400' : ''}
      {info.level === 'warning' ? 'text-yellow-600 dark:text-yellow-400' : ''}
      {info.level === 'ok' ? 'text-green-600 dark:text-green-400' : ''}
      {info.level === 'testing' ? 'text-gray-400' : ''}">
      {#if info.level === 'testing'}
        <svg class="w-3.5 h-3.5 animate-spin" viewBox="0 0 24 24" fill="none">
          <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
          <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z"></path>
        </svg>
      {:else if info.level === 'error'}
        <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 9v2m0 4h.01M12 2a10 10 0 100 20 10 10 0 000-20z" /></svg>
      {:else if info.level === 'warning'}
        <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 9v2m0 4h.01M10.29 3.86L1.82 18a2 2 0 001.71 3h16.94a2 2 0 001.71-3L13.71 3.86a2 2 0 00-3.42 0z" /></svg>
      {:else if info.level === 'ok'}
        <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 13l4 4L19 7" /></svg>
      {/if}
      {info.label}
    </span>
  {/if}
{/snippet}

<div class="space-y-6">
  <div class="flex items-center justify-between">
    {#if modelsSaveMessage}
      <div class="text-sm {modelsSaveMessage.type === 'success' ? 'text-green-600 dark:text-green-400' : 'text-red-600 dark:text-red-400'}">
        {modelsSaveMessage.text}
      </div>
    {:else}
      <div></div>
    {/if}
    <button
      onclick={testAllProviders}
      disabled={recheckRunning || savingModels}
      class="flex items-center gap-1.5 px-3 py-1.5 text-sm border border-gray-300 dark:border-gray-600 rounded-lg
             text-gray-700 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-800
             disabled:opacity-50 disabled:cursor-not-allowed transition-colors cursor-pointer"
    >
      <svg class="w-4 h-4 {recheckRunning ? 'animate-spin' : ''}" fill="none" stroke="currentColor" viewBox="0 0 24 24">
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" />
      </svg>
      {recheckRunning ? 'Checking...' : 'Recheck All'}
    </button>
  </div>

  {#each [
    { key: 'chat', label: 'Chat Providers', providers: config.models.chat.providers, required: true },
    { key: 'embedding', label: 'Embedding Providers', providers: config.models.embedding?.providers ?? [], required: false },
  ] as slot}
    <section>
      <div class="flex items-center gap-2 mb-3">
        <h2 class="text-sm font-semibold text-gray-900 dark:text-gray-100 uppercase tracking-wide">{slot.label}</h2>
        {@render sectionIndicator(slot.key === 'chat' ? chatSectionStatus() : embeddingSectionStatus())}
      </div>

      {#if slot.key === 'embedding'}
        <div class="rounded-lg border border-gray-200 dark:border-gray-800 bg-gray-100/60 dark:bg-gray-800/30 px-4 py-3 mb-2">
          <div class="flex items-center justify-between mb-1">
            <div class="flex items-center gap-2">
              <span class="inline-flex items-center px-1.5 py-0.5 rounded text-[10px] font-semibold uppercase tracking-wider bg-gray-200 dark:bg-gray-700 text-gray-500 dark:text-gray-400">Built-in</span>
              <span class="text-sm font-medium text-gray-500 dark:text-gray-400">Local</span>
              <svg class="w-4 h-4 text-green-500" fill="none" stroke="currentColor" viewBox="0 0 24 24" aria-label="Active"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 13l4 4L19 7" /></svg>
            </div>
          </div>
          <div class="text-sm text-gray-400 dark:text-gray-500 space-y-0.5">
            <div>Model: <span class="text-gray-600 dark:text-gray-400">all-MiniLM-L6-v2</span></div>
            <div>Dimensions: <span class="text-gray-600 dark:text-gray-400">384</span></div>
          </div>
          {#if slot.providers.length === 0}
            <p class="text-xs text-gray-400 dark:text-gray-500 mt-2">Active by default. Add an external provider to override.</p>
          {:else}
            <p class="text-xs text-gray-400 dark:text-gray-500 mt-2">Standby — overridden by the configured provider below.</p>
          {/if}
        </div>
      {/if}

      {#if slot.providers.length > 0}
        <div class="space-y-2">
          {#each slot.providers as provider, i}
            {#if editingProvider?.slot === slot.key && editingProvider?.index === i}
              <!-- Inline edit form -->
              {@render providerForm()}
            {:else}
              <div
                class="group rounded-lg border bg-gray-50 dark:bg-gray-800/50 px-4 py-3 transition-all duration-150
                  {drag?.slot === slot.key && drag.currentIndex === i
                    ? 'border-blue-400 dark:border-blue-500 shadow-lg scale-[1.02] z-10 relative'
                    : 'border-gray-200 dark:border-gray-800'}"
                data-drag-slot={slot.key}
                data-drag-index={i}
              >
                <div class="flex items-center justify-between mb-1">
                  <div class="flex items-center gap-2">
                    <button
                      class="p-0.5 -ml-1 touch-none select-none
                        {slot.providers.length > 1
                          ? 'cursor-grab active:cursor-grabbing text-gray-300 dark:text-gray-600 hover:text-gray-500 dark:hover:text-gray-400'
                          : 'text-gray-200 dark:text-gray-700 cursor-default'}"
                      onpointerdown={(e) => handleDragStart(slot.key, i, e)}
                      aria-label="Drag to reorder"
                    >
                      <svg class="w-4 h-4" viewBox="0 0 16 16" fill="currentColor">
                        <circle cx="5" cy="3" r="1.5"/><circle cx="11" cy="3" r="1.5"/>
                        <circle cx="5" cy="8" r="1.5"/><circle cx="11" cy="8" r="1.5"/>
                        <circle cx="5" cy="13" r="1.5"/><circle cx="11" cy="13" r="1.5"/>
                      </svg>
                    </button>
                    <span class="text-xs font-medium text-gray-400 dark:text-gray-500">{positionLabel(i)}</span>
                    <span class="text-sm font-medium text-gray-900 dark:text-gray-100">{providerLabel(provider.type)}</span>
                    <span title={providerStatus[slot.key + '_' + i]?.message || ''}>
                      {@render statusIcon(providerStatus[slot.key + '_' + i])}
                    </span>
                  </div>
                  <div class="flex items-center gap-1 opacity-0 group-hover:opacity-100 transition-opacity">
                    <button
                      onclick={() => testProviderEntry(slot.key, i, provider)}
                      disabled={recheckRunning || savingModels}
                      class="p-1.5 rounded text-gray-400 hover:text-green-600 dark:hover:text-green-400 hover:bg-gray-200 dark:hover:bg-gray-700 transition-colors cursor-pointer disabled:opacity-30 disabled:cursor-not-allowed"
                      aria-label="Test provider"
                      title="Test connection"
                    >
                      <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M8.111 16.404a5.5 5.5 0 017.778 0M12 20h.01m-7.08-7.071c3.904-3.905 10.236-3.905 14.141 0M1.394 9.393c5.857-5.858 15.355-5.858 21.213 0" />
                      </svg>
                    </button>
                    <button
                      onclick={() => startEdit(slot.key, i)}
                      disabled={savingModels}
                      class="p-1.5 rounded text-gray-400 hover:text-blue-600 dark:hover:text-blue-400 hover:bg-gray-200 dark:hover:bg-gray-700 transition-colors cursor-pointer"
                      aria-label="Edit provider"
                    >
                      <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M11 5H6a2 2 0 00-2 2v11a2 2 0 002 2h11a2 2 0 002-2v-5m-1.414-9.414a2 2 0 112.828 2.828L11.828 15H9v-2.828l8.586-8.586z" />
                      </svg>
                    </button>
                    {#if confirmingDelete?.slot === slot.key && confirmingDelete?.index === i}
                      <button
                        onclick={() => deleteProvider(slot.key, i)}
                        disabled={savingModels}
                        class="px-2 py-1 text-xs font-medium rounded bg-red-600 text-white hover:bg-red-700 transition-colors cursor-pointer disabled:opacity-50"
                      >
                        Confirm
                      </button>
                      <button
                        onclick={() => (confirmingDelete = null)}
                        class="px-2 py-1 text-xs font-medium rounded text-gray-500 hover:text-gray-700 dark:hover:text-gray-300 transition-colors cursor-pointer"
                      >
                        Cancel
                      </button>
                    {:else}
                      <button
                        onclick={() => (confirmingDelete = { slot: slot.key, index: i })}
                        disabled={savingModels || (slot.required && slot.providers.length <= 1)}
                        class="p-1.5 rounded text-gray-400 hover:text-red-600 dark:hover:text-red-400 hover:bg-gray-200 dark:hover:bg-gray-700 transition-colors cursor-pointer disabled:opacity-30 disabled:cursor-not-allowed"
                        aria-label="Delete provider"
                        title={slot.required && slot.providers.length <= 1 ? 'Chat requires at least one provider' : ''}
                      >
                        <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
                        </svg>
                      </button>
                    {/if}
                  </div>
                </div>
                <div class="text-sm text-gray-600 dark:text-gray-400 space-y-0.5">
                  <div>Model: <span class="text-gray-900 dark:text-gray-200">{provider.model}</span></div>
                  {#if provider.endpoint}
                    <div>Endpoint: <span class="text-gray-900 dark:text-gray-200">{provider.endpoint}</span></div>
                  {/if}
                  {#if provider.api_key}
                    <div>API Key: <span class="text-gray-900 dark:text-gray-200">configured</span></div>
                  {:else if provider.api_key_env}
                    <div>API Key: <span class="text-gray-900 dark:text-gray-200">${provider.api_key_env}</span></div>
                  {/if}
                </div>
              </div>
            {/if}
          {/each}
        </div>
      {:else if slot.key !== 'embedding'}
        <p class="text-sm text-gray-500 dark:text-gray-400 italic mb-2">Not configured</p>
      {/if}

      <!-- Add form or button -->
      {#if editingProvider?.slot === slot.key && editingProvider?.index === null}
        <div class="mt-2">
          {@render providerForm()}
        </div>
      {:else}
        <button
          onclick={() => startAdd(slot.key)}
          disabled={savingModels || editingProvider !== null}
          class="mt-2 flex items-center gap-1.5 text-sm text-blue-600 dark:text-blue-400 hover:text-blue-700 dark:hover:text-blue-300 disabled:opacity-40 disabled:cursor-not-allowed transition-colors cursor-pointer"
        >
          <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 4v16m8-8H4" />
          </svg>
          Add Provider
        </button>
      {/if}
    </section>
  {/each}
</div>
