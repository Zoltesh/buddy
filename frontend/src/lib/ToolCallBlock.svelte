<script>
  let { name, arguments: args, result } = $props();
  let expanded = $state(false);

  function formatJson(str) {
    try {
      return JSON.stringify(JSON.parse(str), null, 2);
    } catch {
      return str;
    }
  }

  function humanize(s) {
    return s.replace(/_/g, ' ').replace(/\b\w/g, (l) => l.toUpperCase());
  }
</script>

<div class="my-2 border border-gray-300 dark:border-gray-700 rounded-lg bg-gray-50 dark:bg-gray-800/50 overflow-hidden">
  <button
    class="w-full flex items-center gap-2 px-3 py-2 text-sm text-left hover:bg-gray-100 dark:hover:bg-gray-700/50 transition-colors cursor-pointer"
    onclick={() => (expanded = !expanded)}
  >
    <!-- Cog icon -->
    <svg
      class="w-4 h-4 text-gray-500 dark:text-gray-400 flex-shrink-0"
      fill="none"
      stroke="currentColor"
      viewBox="0 0 24 24"
    >
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

    <span class="font-medium text-gray-700 dark:text-gray-300">{humanize(name)}</span>

    {#if result === null}
      <span class="ml-auto text-xs text-yellow-600 dark:text-yellow-400">running&hellip;</span>
    {:else}
      <span class="ml-auto text-xs text-green-600 dark:text-green-400">done</span>
    {/if}

    <!-- Chevron -->
    <svg
      class="w-3 h-3 text-gray-400 transition-transform duration-150 {expanded
        ? 'rotate-180'
        : ''}"
      fill="none"
      stroke="currentColor"
      viewBox="0 0 24 24"
    >
      <path
        stroke-linecap="round"
        stroke-linejoin="round"
        stroke-width="2"
        d="M19 9l-7 7-7-7"
      />
    </svg>
  </button>

  {#if expanded}
    <div class="px-3 pb-3 border-t border-gray-200 dark:border-gray-700">
      <div class="mt-2">
        <p
          class="text-xs font-semibold text-gray-500 dark:text-gray-400 uppercase tracking-wide mb-1"
        >
          Input
        </p>
        <pre
          class="text-xs bg-gray-100 dark:bg-gray-900 rounded p-2 overflow-x-auto text-gray-800 dark:text-gray-200 max-h-48 overflow-y-auto">{formatJson(args)}</pre>
      </div>
      {#if result !== null}
        <div class="mt-2">
          <p
            class="text-xs font-semibold text-gray-500 dark:text-gray-400 uppercase tracking-wide mb-1"
          >
            Output
          </p>
          <pre
            class="text-xs bg-gray-100 dark:bg-gray-900 rounded p-2 overflow-x-auto text-gray-800 dark:text-gray-200 max-h-48 overflow-y-auto">{formatJson(result)}</pre>
        </div>
      {/if}
    </div>
  {/if}
</div>
