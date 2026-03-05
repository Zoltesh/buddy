<script>
  import { approveTool } from './api.js';
  console.log('ApprovalDialog component loaded');

  let { approval = $bindable() } = $props();

  let isSubmitting = $state(false);

  const permLabel = {
    mutating: 'Mutating',
    network: 'Network',
    read_only: 'Read Only',
  };

  const permBadgeClass = {
    mutating: 'bg-yellow-100 text-yellow-700 dark:bg-yellow-900/30 dark:text-yellow-400',
    network: 'bg-blue-100 text-blue-700 dark:bg-blue-900/30 dark:text-blue-400',
    read_only: 'bg-gray-100 text-gray-600 dark:bg-gray-700 dark:text-gray-300',
  };

  async function handleApprove() {
    if (isSubmitting) return;
    isSubmitting = true;
    try {
      await approveTool(approval.conversationId, approval.id, true);
      approval = null;
    } catch (e) {
      console.error('Failed to approve:', e);
      alert('Failed to approve: ' + e.message);
    } finally {
      isSubmitting = false;
    }
  }

  async function handleDeny() {
    if (isSubmitting) return;
    isSubmitting = true;
    try {
      await approveTool(approval.conversationId, approval.id, false);
      approval = null;
    } catch (e) {
      console.error('Failed to deny:', e);
      alert('Failed to deny: ' + e.message);
    } finally {
      isSubmitting = false;
    }
  }

  function formatArgs(args) {
    try {
      return JSON.stringify(args, null, 2);
    } catch {
      return String(args);
    }
  }
</script>

{#if approval}
  <div class="fixed inset-0 z-50 flex items-center justify-center bg-black/50">
    <div class="bg-white dark:bg-gray-800 rounded-lg shadow-xl max-w-md w-full mx-4 p-6">
      <h2 class="text-lg font-semibold text-gray-900 dark:text-gray-100 mb-2">
        Tool Approval Request
      </h2>

      <div class="mb-4">
        <div class="flex items-center gap-2 mb-2">
          <span class="font-medium text-gray-900 dark:text-gray-100">{approval.skill_name}</span>
          <span class="text-xs font-medium px-2 py-0.5 rounded-full {permBadgeClass[approval.permission_level] || ''}">
            {permLabel[approval.permission_level] || approval.permission_level}
          </span>
        </div>
        <p class="text-sm text-gray-600 dark:text-gray-400">
          This tool requires permission to execute.
        </p>
      </div>

      <div class="mb-4">
        <p class="text-xs font-medium text-gray-500 dark:text-gray-400 mb-1">Arguments:</p>
        <pre class="text-xs bg-gray-100 dark:bg-gray-900 rounded p-2 overflow-x-auto max-h-40">{formatArgs(approval.arguments)}</pre>
      </div>

      <div class="flex gap-3 justify-end">
        <button
          type="button"
          onclick={handleDeny}
          disabled={isSubmitting}
          class="px-4 py-2 text-sm font-medium text-gray-700 dark:text-gray-300 bg-gray-100 dark:bg-gray-700 rounded-lg hover:bg-gray-200 dark:hover:bg-gray-600 disabled:opacity-50 disabled:cursor-not-allowed"
        >
          Deny
        </button>
        <button
          type="button"
          onclick={handleApprove}
          disabled={isSubmitting}
          class="px-4 py-2 text-sm font-medium text-white bg-blue-600 rounded-lg hover:bg-blue-700 disabled:opacity-50 disabled:cursor-not-allowed"
        >
          {isSubmitting ? 'Sending...' : 'Approve'}
        </button>
      </div>
    </div>
  </div>
{/if}
