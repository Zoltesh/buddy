<script>
  import { onMount, onDestroy } from 'svelte';
  import { fetchConfig, putConfigChat, putConfigMemory, putConfigModels, putConfigSkills, testProvider } from './api.js';

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

  // Models tab state
  // editingProvider: { slot: 'chat'|'embedding', index: number|null, form: {...} }
  // index === null means adding new; number means editing existing
  let editingProvider = $state(null);
  let modelErrors = $state({});
  let testing = $state(false);
  let testResult = $state(null);
  let savingModels = $state(false);
  let modelsSaveMessage = $state(null);
  let confirmingDelete = $state(null); // { slot, index }

  // Drag-and-drop reorder state
  let drag = $state(null); // { slot, fromIndex, currentIndex, snapshot }

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
    } catch (e) {
      if (slot === 'chat') {
        config.models.chat.providers = snapshot;
      } else if (config.models.embedding) {
        config.models.embedding.providers = snapshot;
      }
      if (e.details?.errors) {
        modelsSaveMessage = { type: 'error', text: e.details.errors.map(err => `${err.field}: ${err.message}`).join('; ') };
      } else {
        modelsSaveMessage = { type: 'error', text: e.details?.message || e.message };
      }
    } finally {
      savingModels = false;
    }
  }

  onDestroy(() => {
    document.removeEventListener('pointermove', onDragPointerMove);
    document.removeEventListener('pointerup', onDragPointerUp);
  });

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
    return type === 'openai' || type === 'lmstudio';
  }

  function showsEndpoint(type) {
    return type !== 'local';
  }

  function providerLabel(type) {
    return providerTypes.find((p) => p.value === type)?.label ?? type;
  }

  function positionLabel(index) {
    return index === 0 ? 'Primary' : `Fallback #${index + 1}`;
  }

  function startAdd(slot) {
    editingProvider = { slot, index: null, form: { type: 'openai', model: '', endpoint: '', api_key_env: '' } };
    modelErrors = {};
    testResult = null;
    modelsSaveMessage = null;
  }

  function startEdit(slot, index) {
    const p = slot === 'chat'
      ? config.models.chat.providers[index]
      : config.models.embedding.providers[index];
    editingProvider = { slot, index, form: { type: p.type, model: p.model, endpoint: p.endpoint ?? '', api_key_env: p.api_key_env ?? '' } };
    modelErrors = {};
    testResult = null;
    modelsSaveMessage = null;
  }

  function cancelEdit() {
    editingProvider = null;
    modelErrors = {};
    testResult = null;
  }

  function validateForm(form) {
    const errors = {};
    if (!form.model.trim()) errors.model = 'Model name is required';
    return errors;
  }

  function buildModelsPayload(slot, providers) {
    const chat = slot === 'chat' ? { providers } : { providers: config.models.chat.providers };
    const embedding = slot === 'embedding'
      ? (providers.length > 0 ? { providers } : null)
      : (config.models.embedding ?? null);
    const payload = { chat };
    if (embedding) payload.embedding = embedding;
    return payload;
  }

  function toEntry(form) {
    const entry = { type: form.type, model: form.model.trim() };
    if (showsEndpoint(form.type) && form.endpoint.trim()) entry.endpoint = form.endpoint.trim();
    if (showsApiKey(form.type) && form.api_key_env.trim()) entry.api_key_env = form.api_key_env.trim();
    return entry;
  }

  async function saveProvider() {
    const { slot, index, form } = editingProvider;
    const errors = validateForm(form);
    if (Object.keys(errors).length > 0) { modelErrors = errors; return; }

    savingModels = true;
    modelsSaveMessage = null;
    try {
      const currentProviders = slot === 'chat'
        ? [...config.models.chat.providers]
        : [...(config.models.embedding?.providers ?? [])];

      const entry = toEntry(form);
      if (index === null) {
        currentProviders.push(entry);
      } else {
        currentProviders[index] = entry;
      }

      const updated = await putConfigModels(buildModelsPayload(slot, currentProviders));
      config = updated;
      editingProvider = null;
      modelErrors = {};
      testResult = null;
      modelsSaveMessage = { type: 'success', text: 'Provider saved.' };
    } catch (e) {
      if (e.details?.errors) {
        modelsSaveMessage = { type: 'error', text: e.details.errors.map((err) => `${err.field}: ${err.message}`).join('; ') };
      } else {
        modelsSaveMessage = { type: 'error', text: e.details?.message || e.message };
      }
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
      confirmingDelete = null;
      return;
    }

    savingModels = true;
    modelsSaveMessage = null;
    try {
      currentProviders.splice(index, 1);
      const updated = await putConfigModels(buildModelsPayload(slot, currentProviders));
      config = updated;
      confirmingDelete = null;
      if (editingProvider?.slot === slot && editingProvider?.index === index) {
        editingProvider = null;
      }
      modelsSaveMessage = { type: 'success', text: 'Provider removed.' };
    } catch (e) {
      if (e.details?.errors) {
        modelsSaveMessage = { type: 'error', text: e.details.errors.map((err) => `${err.field}: ${err.message}`).join('; ') };
      } else {
        modelsSaveMessage = { type: 'error', text: e.details?.message || e.message };
      }
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
      if (e.details?.errors) {
        testResult = { status: 'error', message: e.details.errors.map((err) => err.message).join('; ') };
      } else {
        testResult = { status: 'error', message: e.message };
      }
    } finally {
      testing = false;
    }
  }

  const tabs = [
    { id: 'general', label: 'General' },
    { id: 'models', label: 'Models' },
    { id: 'skills', label: 'Skills' },
  ];

  // Skills tab state
  let skillsSaving = $state(false);
  let skillsSaveMessage = $state(null);
  let skillErrors = $state({});
  let skillInputs = $state({ read_file: '', write_file: '', fetch_url: '' });

  const skillDefs = [
    { key: 'read_file', label: 'Read File', permission: 'ReadOnly', field: 'allowed_directories', fieldLabel: 'Allowed directories', placeholder: '/path/to/directory' },
    { key: 'write_file', label: 'Write File', permission: 'Mutating', field: 'allowed_directories', fieldLabel: 'Allowed directories', placeholder: '/path/to/directory' },
    { key: 'fetch_url', label: 'Fetch URL', permission: 'Network', field: 'allowed_domains', fieldLabel: 'Allowed domains', placeholder: 'example.com' },
  ];

  const permissionBadgeClass = {
    ReadOnly: 'bg-gray-100 text-gray-600 dark:bg-gray-700 dark:text-gray-300',
    Mutating: 'bg-yellow-100 text-yellow-700 dark:bg-yellow-900/30 dark:text-yellow-400',
    Network: 'bg-blue-100 text-blue-700 dark:bg-blue-900/30 dark:text-blue-400',
  };

  function toggleSkill(key) {
    if (config.skills[key]) {
      config.skills[key] = null;
    } else {
      const def = skillDefs.find(s => s.key === key);
      if (def.field === 'allowed_directories') {
        config.skills[key] = { allowed_directories: [] };
      } else {
        config.skills[key] = { allowed_domains: [] };
      }
    }
  }

  function addEntry(key, field) {
    const value = skillInputs[key].trim();
    if (!value) return;
    config.skills[key][field].push(value);
    config.skills[key] = { ...config.skills[key] };
    skillInputs[key] = '';
    // Clear any error for this skill
    delete skillErrors[key];
    skillErrors = { ...skillErrors };
  }

  function removeEntry(key, field, index) {
    config.skills[key][field].splice(index, 1);
    config.skills[key] = { ...config.skills[key] };
  }

  async function saveSkills() {
    // Client-side validation: check for empty strings in enabled skills' lists
    const errors = {};
    for (const def of skillDefs) {
      const skillConfig = config.skills[def.key];
      if (!skillConfig) continue;
      const entries = skillConfig[def.field] ?? [];
      for (let i = 0; i < entries.length; i++) {
        if (!entries[i].trim()) {
          errors[def.key] = 'Entries must not be empty';
          break;
        }
      }
    }
    if (Object.keys(errors).length > 0) {
      skillErrors = errors;
      return;
    }
    skillErrors = {};

    skillsSaving = true;
    skillsSaveMessage = null;
    try {
      const payload = {
        read_file: config.skills.read_file ?? null,
        write_file: config.skills.write_file ?? null,
        fetch_url: config.skills.fetch_url ?? null,
      };
      const updated = await putConfigSkills(payload);
      config = updated;
      skillsSaveMessage = { type: 'success', text: 'Skills saved.' };
    } catch (e) {
      const details = e.details;
      if (details?.errors) {
        skillsSaveMessage = {
          type: 'error',
          text: details.errors.map((err) => `${err.field}: ${err.message}`).join('; '),
        };
      } else {
        skillsSaveMessage = { type: 'error', text: details?.message || e.message };
      }
    } finally {
      skillsSaving = false;
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
              placeholder="e.g. gpt-4o, deepseek-coder"
              class="w-full px-3 py-2 border rounded-lg bg-white dark:bg-gray-800 text-gray-900 dark:text-gray-100
                     focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-transparent text-sm
                     {modelErrors.model ? 'border-red-400 dark:border-red-600' : 'border-gray-300 dark:border-gray-700'}"
            />
            {#if modelErrors.model}
              <p class="text-xs text-red-600 dark:text-red-400 mt-1">{modelErrors.model}</p>
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
              <label class="block text-sm text-gray-700 dark:text-gray-300 mb-1" for="pf-apikey">API Key Env Var</label>
              <input
                id="pf-apikey"
                type="text"
                bind:value={editingProvider.form.api_key_env}
                placeholder="e.g. OPENAI_API_KEY"
                class="w-full px-3 py-2 border border-gray-300 dark:border-gray-700 rounded-lg
                       bg-white dark:bg-gray-800 text-gray-900 dark:text-gray-100
                       focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-transparent text-sm"
              />
              <p class="text-xs text-gray-400 dark:text-gray-500 mt-1">Name of the environment variable (not the key itself)</p>
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
            {#if modelsSaveMessage}
              <div class="text-sm {modelsSaveMessage.type === 'success' ? 'text-green-600 dark:text-green-400' : 'text-red-600 dark:text-red-400'}">
                {modelsSaveMessage.text}
              </div>
            {/if}

            {#each [
              { key: 'chat', label: 'Chat Providers', providers: config.models.chat.providers, required: true },
              { key: 'embedding', label: 'Embedding Providers', providers: config.models.embedding?.providers ?? [], required: false },
            ] as slot}
              <section>
                <h2 class="text-sm font-semibold text-gray-900 dark:text-gray-100 uppercase tracking-wide mb-3">{slot.label}</h2>

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
                            </div>
                            <div class="flex items-center gap-1 opacity-0 group-hover:opacity-100 transition-opacity">
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
                            {#if provider.api_key_env}
                              <div>API Key: <span class="text-gray-900 dark:text-gray-200">${provider.api_key_env}</span></div>
                            {/if}
                          </div>
                        </div>
                      {/if}
                    {/each}
                  </div>
                {:else}
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

        <!-- Skills Tab -->
        {:else if activeTab === 'skills'}
          <div class="space-y-6">
            {#each skillDefs as skill}
              <section>
                <div class="rounded-lg border border-gray-200 dark:border-gray-800 bg-gray-50 dark:bg-gray-800/50 px-4 py-3">
                  <!-- Header: name, badge, toggle -->
                  <div class="flex items-center justify-between">
                    <div class="flex items-center gap-2">
                      <h2 class="text-sm font-semibold text-gray-900 dark:text-gray-100">{skill.label}</h2>
                      <span class="text-xs font-medium px-2 py-0.5 rounded-full {permissionBadgeClass[skill.permission]}">{skill.permission}</span>
                    </div>
                    <label class="relative inline-flex items-center cursor-pointer">
                      <input
                        type="checkbox"
                        checked={config.skills[skill.key] != null}
                        onchange={() => toggleSkill(skill.key)}
                        class="sr-only peer"
                      />
                      <div class="w-9 h-5 bg-gray-300 dark:bg-gray-600 peer-focus:outline-none peer-focus:ring-2 peer-focus:ring-blue-500 rounded-full peer peer-checked:bg-blue-600 transition-colors"></div>
                      <div class="absolute left-0.5 top-0.5 bg-white w-4 h-4 rounded-full transition-transform peer-checked:translate-x-4"></div>
                    </label>
                  </div>

                  <!-- Body (shown when enabled) -->
                  {#if config.skills[skill.key]}
                    <div class="mt-3 space-y-3">
                      <!-- Sandbox entries list -->
                      <div>
                        <span class="block text-sm text-gray-600 dark:text-gray-400 mb-1">{skill.fieldLabel}</span>
                        {#if config.skills[skill.key][skill.field]?.length > 0}
                          <ul class="space-y-1 mb-2">
                            {#each config.skills[skill.key][skill.field] as entry, i}
                              <li class="flex items-center gap-2 text-sm text-gray-900 dark:text-gray-200">
                                <span class="flex-1 px-2 py-1 bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded">{entry}</span>
                                <button
                                  onclick={() => removeEntry(skill.key, skill.field, i)}
                                  class="p-1 text-gray-400 hover:text-red-600 dark:hover:text-red-400 transition-colors cursor-pointer"
                                  aria-label="Remove entry"
                                >
                                  <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
                                  </svg>
                                </button>
                              </li>
                            {/each}
                          </ul>
                        {:else}
                          <p class="text-xs text-gray-400 dark:text-gray-500 italic mb-2">No entries configured</p>
                        {/if}

                        <!-- Add entry input -->
                        <div class="flex gap-2">
                          <input
                            type="text"
                            bind:value={skillInputs[skill.key]}
                            placeholder={skill.placeholder}
                            onkeydown={(e) => { if (e.key === 'Enter') addEntry(skill.key, skill.field); }}
                            class="flex-1 px-3 py-1.5 border border-gray-300 dark:border-gray-700 rounded-lg
                                   bg-white dark:bg-gray-800 text-gray-900 dark:text-gray-100
                                   focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-transparent text-sm"
                          />
                          <button
                            onclick={() => addEntry(skill.key, skill.field)}
                            class="px-3 py-1.5 text-sm border border-gray-300 dark:border-gray-600 rounded-lg
                                   text-gray-700 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-800
                                   transition-colors cursor-pointer"
                          >
                            Add
                          </button>
                        </div>
                        {#if skillErrors[skill.key]}
                          <p class="text-xs text-red-600 dark:text-red-400 mt-1">{skillErrors[skill.key]}</p>
                        {/if}
                      </div>

                      <!-- Approval policy (only for Mutating/Network) -->
                      {#if skill.permission !== 'ReadOnly'}
                        <div>
                          <label class="block text-sm text-gray-600 dark:text-gray-400 mb-1" for="approval-{skill.key}">Approval policy</label>
                          <select
                            id="approval-{skill.key}"
                            value={config.skills[skill.key].approval ?? 'always'}
                            onchange={(e) => { config.skills[skill.key] = { ...config.skills[skill.key], approval: e.target.value }; }}
                            class="w-48 px-3 py-1.5 border border-gray-300 dark:border-gray-700 rounded-lg
                                   bg-white dark:bg-gray-800 text-gray-900 dark:text-gray-100
                                   focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-transparent text-sm cursor-pointer"
                          >
                            <option value="always">Always ask</option>
                            <option value="once">Ask once per conversation</option>
                            <option value="trust">Trust (auto-approve)</option>
                          </select>
                        </div>
                      {/if}
                    </div>
                  {/if}
                </div>
              </section>
            {/each}

            <!-- Save button + message -->
            <div class="flex items-center gap-3 pt-2">
              <button
                onclick={saveSkills}
                disabled={skillsSaving}
                class="px-6 py-2 bg-blue-600 text-white rounded-lg font-medium hover:bg-blue-700
                       focus:outline-none focus:ring-2 focus:ring-blue-500 focus:ring-offset-2
                       disabled:opacity-50 disabled:cursor-not-allowed transition-colors cursor-pointer"
              >
                {skillsSaving ? 'Saving...' : 'Save'}
              </button>
              {#if skillsSaveMessage}
                <span
                  class="text-sm {skillsSaveMessage.type === 'success'
                    ? 'text-green-600 dark:text-green-400'
                    : 'text-red-600 dark:text-red-400'}"
                >
                  {skillsSaveMessage.text}
                </span>
              {/if}
            </div>
          </div>
        {/if}
      {/if}
    </div>
  </div>
</div>
