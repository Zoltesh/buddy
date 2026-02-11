<script>
  import { verifyToken, setAuthToken } from './api.js';

  let { onAuthenticated } = $props();

  let token = $state('');
  let error = $state('');
  let submitting = $state(false);

  async function handleSubmit() {
    if (!token.trim() || submitting) return;
    submitting = true;
    error = '';
    try {
      const result = await verifyToken(token.trim());
      if (result.valid) {
        setAuthToken(token.trim());
        onAuthenticated();
      } else {
        error = 'Invalid token. Please try again.';
        token = '';
      }
    } catch {
      error = 'Invalid token. Please try again.';
      token = '';
    } finally {
      submitting = false;
    }
  }
</script>

<div class="flex items-center justify-center min-h-screen bg-gray-50 dark:bg-gray-950 px-4">
  <div class="w-full max-w-sm bg-white dark:bg-gray-900 rounded-xl shadow-lg border border-gray-200 dark:border-gray-800 p-8">
    <!-- Branding -->
    <div class="text-center mb-8">
      <h1 class="text-3xl font-bold text-blue-600 dark:text-blue-400">buddy</h1>
      <p class="mt-2 text-sm text-gray-500 dark:text-gray-400">Enter your access token to continue</p>
    </div>

    <!-- Login form -->
    <form
      onsubmit={(e) => {
        e.preventDefault();
        handleSubmit();
      }}
      class="space-y-4"
    >
      <div>
        <label for="auth-token" class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">
          Access Token
        </label>
        <input
          id="auth-token"
          type="password"
          bind:value={token}
          disabled={submitting}
          placeholder="Enter your token"
          class="w-full px-4 py-2 border border-gray-300 dark:border-gray-700 rounded-lg
                 bg-white dark:bg-gray-800 text-gray-900 dark:text-gray-100
                 placeholder-gray-400 dark:placeholder-gray-500
                 focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-transparent
                 disabled:opacity-50 disabled:cursor-not-allowed"
        />
      </div>

      {#if error}
        <p class="text-sm text-red-600 dark:text-red-400">{error}</p>
      {/if}

      <button
        type="submit"
        disabled={submitting || !token.trim()}
        class="w-full px-4 py-2 bg-blue-600 text-white rounded-lg font-medium
               hover:bg-blue-700 focus:outline-none focus:ring-2 focus:ring-blue-500 focus:ring-offset-2
               disabled:opacity-50 disabled:cursor-not-allowed disabled:hover:bg-blue-600
               transition-colors cursor-pointer"
      >
        {submitting ? 'Verifying...' : 'Sign In'}
      </button>
    </form>
  </div>
</div>
