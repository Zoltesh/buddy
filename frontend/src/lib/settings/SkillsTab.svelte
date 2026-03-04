<script>
  let { config = $bindable() } = $props();

  const skills = [
    {
      key: 'remember',
      label: 'Remember',
      description: 'Save a fact, preference, or important information to long-term memory for later retrieval across conversations.',
      tools: ['memory_write'],
    },
    {
      key: 'recall',
      label: 'Recall',
      description: 'Search long-term memory for previously stored facts, preferences, or context relevant to a query.',
      tools: ['memory_read'],
    },
  ];
</script>

<div class="space-y-6">
  <div class="rounded-lg border border-gray-200 dark:border-gray-800 bg-gray-50 dark:bg-gray-800/50 px-4 py-3">
    <div class="flex items-center gap-2 mb-2">
      <h2 class="text-sm font-semibold text-gray-900 dark:text-gray-100">Skills</h2>
      <span class="inline-flex items-center gap-1 text-xs font-medium text-green-600 dark:text-green-400">
        <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 13l4 4L19 7" /></svg>
        Available
      </span>
    </div>
    <p class="text-sm text-gray-600 dark:text-gray-400">
      Skills are composite AI capabilities that combine multiple tools for complex operations.
      Configure individual tools in the Tools tab.
    </p>
  </div>

  <div class="space-y-4">
    {#each skills as skill}
      <section>
        <div class="rounded-lg border border-gray-200 dark:border-gray-800 bg-gray-50 dark:bg-gray-800/50 px-4 py-3">
          <div class="flex items-center gap-2 mb-2">
            <h3 class="text-sm font-semibold text-gray-900 dark:text-gray-100">{skill.label}</h3>
          </div>
          <p class="text-sm text-gray-600 dark:text-gray-400 mb-2">{skill.description}</p>
          <div class="flex items-center gap-2 text-xs text-gray-500 dark:text-gray-400">
            <span>Uses:</span>
            {#each skill.tools as tool, i}
              <span class="px-2 py-0.5 bg-gray-200 dark:bg-gray-700 rounded">{tool}</span>
              {#if i < skill.tools.length - 1}<span class="text-gray-400">+</span>{/if}
            {/each}
          </div>
        </div>
      </section>
    {/each}
  </div>
</div>
