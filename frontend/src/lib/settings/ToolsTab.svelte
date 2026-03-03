<script>
  let { config = $bindable() } = $props();

  const toolDefs = [
    {
      key: 'read_file',
      label: 'Read File',
      description: 'Read contents of files from allowed directories.',
      permission: 'ReadOnly',
    },
    {
      key: 'write_file',
      label: 'Write File',
      description: 'Write content to files in allowed directories.',
      permission: 'Mutating',
    },
    {
      key: 'fetch_url',
      label: 'Fetch URL',
      description: 'Fetch the contents of a URL via HTTP GET.',
      permission: 'Network',
    },
    {
      key: 'memory_read',
      label: 'Memory Read',
      description: "Read from the conversation's working memory scratchpad.",
      permission: 'ReadOnly',
    },
    {
      key: 'memory_write',
      label: 'Memory Write',
      description: "Write to the conversation's working memory scratchpad.",
      permission: 'Mutating',
    },
  ];

  const permissionBadgeClass = {
    ReadOnly: 'bg-gray-100 text-gray-600 dark:bg-gray-700 dark:text-gray-300',
    Mutating: 'bg-yellow-100 text-yellow-700 dark:bg-yellow-900/30 dark:text-yellow-400',
    Network: 'bg-blue-100 text-blue-700 dark:bg-blue-900/30 dark:text-blue-400',
  };

  function isToolEnabled(key) {
    return config?.skills?.[key] != null;
  }
</script>

<div class="space-y-4">
  <p class="text-sm text-gray-600 dark:text-gray-400">
    Available tools (atomic capabilities). Configure sandbox rules in the Skills tab.
  </p>

  {#each toolDefs as tool}
    <section>
      <div class="rounded-lg border border-gray-200 dark:border-gray-800 bg-gray-50 dark:bg-gray-800/50 px-4 py-3">
        <div class="flex items-center justify-between">
          <div class="flex items-center gap-2">
            <h2 class="text-sm font-semibold text-gray-900 dark:text-gray-100">{tool.label}</h2>
            <span class="text-xs font-medium px-2 py-0.5 rounded-full {permissionBadgeClass[tool.permission]}">{tool.permission}</span>
            {#if isToolEnabled(tool.key)}
              <span class="inline-flex items-center gap-1 text-xs font-medium text-green-600 dark:text-green-400">
                <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 13l4 4L19 7" /></svg>
                Enabled
              </span>
            {:else}
              <span class="inline-flex items-center gap-1 text-xs font-medium text-gray-400">
                <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" /></svg>
                Disabled
              </span>
            {/if}
          </div>
        </div>
        <p class="mt-1 text-sm text-gray-600 dark:text-gray-400">{tool.description}</p>
      </div>
    </section>
  {/each}
</div>
