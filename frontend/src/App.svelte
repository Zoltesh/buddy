<script>
  import { marked } from 'marked';
  import { onMount } from 'svelte';

  // Configure marked for safe rendering
  marked.setOptions({
    breaks: true,
    gfm: true,
  });

  let messages = $state([]);
  let inputText = $state('');
  let isStreaming = $state(false);
  let messagesContainer;

  // Auto-scroll to bottom when messages change
  $effect(() => {
    if (messagesContainer && messages.length > 0) {
      messagesContainer.scrollTop = messagesContainer.scrollHeight;
    }
  });

  async function sendMessage() {
    const text = inputText.trim();
    if (!text || isStreaming) return;

    // Add user message
    const userMessage = {
      role: 'user',
      content: text,
      timestamp: new Date().toISOString(),
    };
    messages.push(userMessage);
    inputText = '';
    isStreaming = true;

    // Create assistant message placeholder and track its index
    // (must access via messages[idx] for Svelte 5 proxy reactivity)
    messages.push({
      role: 'assistant',
      content: '',
      timestamp: new Date().toISOString(),
    });
    const assistantIdx = messages.length - 1;

    try {
      // Build request body matching backend Message schema
      const requestBody = {
        messages: messages
          .filter((m) => m.role !== 'assistant' || m.content.trim())
          .map((m) => ({
            role: m.role,
            content: { type: 'text', text: m.content },
            timestamp: m.timestamp,
          })),
      };

      // Start SSE connection
      const response = await fetch('/api/chat', {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
        },
        body: JSON.stringify(requestBody),
      });

      if (!response.ok) {
        const error = await response.json();
        messages[assistantIdx].content = `Error: ${error.message || 'Request failed'}`;
        isStreaming = false;
        return;
      }

      // Parse SSE stream
      const reader = response.body.getReader();
      const decoder = new TextDecoder();
      let buffer = '';

      while (true) {
        const { done, value } = await reader.read();
        if (done) break;

        buffer += decoder.decode(value, { stream: true });
        const lines = buffer.split('\n');
        buffer = lines.pop() || ''; // Keep incomplete line in buffer

        for (const line of lines) {
          if (line.startsWith('data: ')) {
            const data = line.slice(6);
            try {
              const event = JSON.parse(data);

              if (event.type === 'token_delta') {
                messages[assistantIdx].content += event.content;
              } else if (event.type === 'error') {
                messages[assistantIdx].content += `\n\nError: ${event.message}`;
              } else if (event.type === 'done') {
                // Stream complete
              }
            } catch (e) {
              console.error('Failed to parse SSE event:', e, data);
            }
          }
        }
      }
    } catch (error) {
      console.error('Chat error:', error);
      messages[assistantIdx].content = `Error: ${error.message}`;
    } finally {
      isStreaming = false;
    }
  }

  function handleKeydown(event) {
    if (event.key === 'Enter' && !event.shiftKey) {
      event.preventDefault();
      sendMessage();
    }
  }

  function renderMarkdown(content) {
    return marked.parse(content);
  }
</script>

<div class="flex flex-col h-screen max-w-4xl mx-auto bg-white dark:bg-gray-900">
  <!-- Header -->
  <header class="flex-shrink-0 border-b border-gray-200 dark:border-gray-800 px-4 py-3">
    <h1 class="text-xl font-semibold text-gray-900 dark:text-gray-100">buddy</h1>
  </header>

  <!-- Messages container -->
  <div
    bind:this={messagesContainer}
    class="flex-1 overflow-y-auto px-4 py-6 space-y-4"
  >
    {#each messages as message}
      <div
        class="flex {message.role === 'user' ? 'justify-end' : 'justify-start'}"
      >
        <div
          class="max-w-[80%] rounded-lg px-4 py-3 {message.role === 'user'
            ? 'bg-blue-600 text-white'
            : 'bg-gray-100 dark:bg-gray-800 text-gray-900 dark:text-gray-100'}"
        >
          {#if message.role === 'user'}
            <p class="whitespace-pre-wrap break-words">{message.content}</p>
          {:else}
            <div class="prose prose-sm dark:prose-invert max-w-none prose-pre:bg-gray-900 prose-pre:text-gray-100">
              {@html renderMarkdown(message.content || '...')}
            </div>
          {/if}
        </div>
      </div>
    {/each}

    {#if isStreaming}
      <div class="flex justify-start">
        <div class="bg-gray-100 dark:bg-gray-800 rounded-lg px-4 py-3">
          <div class="flex space-x-2">
            <div class="w-2 h-2 bg-gray-400 rounded-full animate-bounce" style="animation-delay: 0ms;"></div>
            <div class="w-2 h-2 bg-gray-400 rounded-full animate-bounce" style="animation-delay: 150ms;"></div>
            <div class="w-2 h-2 bg-gray-400 rounded-full animate-bounce" style="animation-delay: 300ms;"></div>
          </div>
        </div>
      </div>
    {/if}
  </div>

  <!-- Input area -->
  <div class="flex-shrink-0 border-t border-gray-200 dark:border-gray-800 px-4 py-4">
    <form onsubmit={(e) => { e.preventDefault(); sendMessage(); }} class="flex gap-2">
      <input
        type="text"
        bind:value={inputText}
        onkeydown={handleKeydown}
        disabled={isStreaming}
        placeholder="Type a message..."
        class="flex-1 px-4 py-2 border border-gray-300 dark:border-gray-700 rounded-lg
               bg-white dark:bg-gray-800 text-gray-900 dark:text-gray-100
               placeholder-gray-500 dark:placeholder-gray-400
               focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-transparent
               disabled:opacity-50 disabled:cursor-not-allowed"
      />
      <button
        type="submit"
        disabled={isStreaming || !inputText.trim()}
        class="px-6 py-2 bg-blue-600 text-white rounded-lg font-medium
               hover:bg-blue-700 focus:outline-none focus:ring-2 focus:ring-blue-500 focus:ring-offset-2
               disabled:opacity-50 disabled:cursor-not-allowed disabled:hover:bg-blue-600
               transition-colors"
      >
        Send
      </button>
    </form>
  </div>
</div>
