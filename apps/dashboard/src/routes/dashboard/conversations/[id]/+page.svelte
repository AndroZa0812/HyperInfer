<script lang="ts">
    import { page } from '$app/stores';
    import { api } from '$lib/api';
    import { onMount } from 'svelte';
    import VirtualList from '$lib/components/VirtualList.svelte';

    let conversation = $state<any>(null);
    let loading = $state(true);

    let conversationId = $derived($page.params.id);

    onMount(async () => {
        if (conversationId) {
            try {
                conversation = await api.getConversation(conversationId);
            } catch (e) {
                console.error('Failed to load conversation', e);
            } finally {
                loading = false;
            }
        }
    });
</script>

{#if loading}
    <p>Loading...</p>
{:else if conversation}
    <div class="h-full flex flex-col">
        <div class="flex items-center justify-between mb-4">
            <h1 class="text-2xl font-bold">{conversation.title || 'Conversation'}</h1>
        </div>

        <div class="flex-1 bg-[var(--bg-primary)] rounded-xl overflow-hidden">
            <VirtualList items={conversation.items || []} let:item>
                <div class="p-4 border-b border-[var(--bg-secondary)] {item.direction === 'input' ? 'text-right bg-gray-50 dark:bg-gray-800' : ''}">
                    <div class="inline-block max-w-[70%]">
                        <span class="text-xs text-gray-500 mb-1 block">
                            {item.direction === 'input' ? 'User' : 'Assistant'}
                        </span>
                        <p class="whitespace-pre-wrap">{item.content?.text || item.content}</p>
                    </div>
                </div>
            </VirtualList>
        </div>
    </div>
{:else}
    <p>Conversation not found</p>
{/if}