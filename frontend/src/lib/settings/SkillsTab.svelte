<script>
  import { formatApiError, putConfigSkills } from '../api.js';

  let { config = $bindable() } = $props();

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

  function skillWarning(key) {
    const skillConfig = config?.skills[key];
    if (!skillConfig) return null;
    const def = skillDefs.find(s => s.key === key);
    const entries = skillConfig[def.field] ?? [];
    if (entries.length === 0) return { level: 'warning', label: 'No sandbox rules' };
    return null;
  }

  function toggleSkill(key) {
    if (config.skills[key]) {
      config.skills[key] = null;
    } else {
      const def = skillDefs.find(s => s.key === key);
      config.skills = {
        ...config.skills,
        [key]: { [def.field]: [], ...(def.permission !== 'ReadOnly' ? { approval: 'always' } : {}) },
      };
    }
  }

  function addEntry(key, field) {
    const value = (skillInputs[key] ?? '').trim();
    if (!value) return;
    const current = config.skills[key][field] ?? [];
    if (current.includes(value)) {
      skillErrors = { ...skillErrors, [key]: `"${value}" already exists` };
      return;
    }
    config.skills[key] = { ...config.skills[key], [field]: [...current, value] };
    skillInputs = { ...skillInputs, [key]: '' };
    skillErrors = { ...skillErrors, [key]: null };
  }

  function removeEntry(key, field, index) {
    const current = [...(config.skills[key][field] ?? [])];
    current.splice(index, 1);
    config.skills[key] = { ...config.skills[key], [field]: current };
  }

  async function saveSkills() {
    skillsSaving = true;
    skillsSaveMessage = null;
    skillErrors = {};
    const payload = {};
    for (const def of skillDefs) {
      if (config.skills[def.key]) {
        const entry = { ...config.skills[def.key] };
        if (entry[def.field]?.length === 0) {
          skillErrors = { ...skillErrors, [def.key]: `${def.fieldLabel} must not be empty when enabled` };
        }
        payload[def.key] = entry;
      }
    }
    if (Object.values(skillErrors).some(Boolean)) {
      skillsSaving = false;
      skillsSaveMessage = { type: 'error', text: 'Fix validation errors before saving.' };
      return;
    }
    try {
      const updated = await putConfigSkills(payload);
      config = updated;
      skillsSaveMessage = { type: 'success', text: 'Skills saved.' };
    } catch (e) {
      skillsSaveMessage = { type: 'error', text: formatApiError(e) };
    } finally {
      skillsSaving = false;
    }
  }
</script>

<div class="space-y-6">
  {#each skillDefs as skill}
    <section>
      <div class="rounded-lg border border-gray-200 dark:border-gray-800 bg-gray-50 dark:bg-gray-800/50 px-4 py-3">
        <!-- Header: name, badge, toggle -->
        <div class="flex items-center justify-between">
          <div class="flex items-center gap-2">
            <h2 class="text-sm font-semibold text-gray-900 dark:text-gray-100">{skill.label}</h2>
            <span class="text-xs font-medium px-2 py-0.5 rounded-full {permissionBadgeClass[skill.permission]}">{skill.permission}</span>
            {#snippet sectionIndicator(info)}
              {#if info}
                <span class="inline-flex items-center gap-1 text-xs font-medium
                  {info.level === 'error' ? 'text-red-600 dark:text-red-400' : ''}
                  {info.level === 'warning' ? 'text-yellow-600 dark:text-yellow-400' : ''}
                  {info.level === 'ok' ? 'text-green-600 dark:text-green-400' : ''}
                  {info.level === 'testing' ? 'text-gray-400' : ''}">
                  {#if info.level === 'error'}
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
            {@render sectionIndicator(skillWarning(skill.key))}
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
