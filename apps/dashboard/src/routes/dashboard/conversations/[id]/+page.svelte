<script lang="ts">
    import { page } from '$app/stores';
    import { api } from '$lib/api';
    import { onMount } from 'svelte';
    import VirtualList from '$lib/components/VirtualList.svelte';
    import Card from '$lib/components/Card.svelte';
    import Button from '$lib/components/Button.svelte';
    import Badge from '$lib/components/Badge.svelte';

    interface ConversationItem {
        direction: 'input' | 'output';
        content: { text?: string } | string;
    }

    let conversation = $state<{ title?: string; items?: ConversationItem[] } | null>(null);
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
    <div class="flex items-center justify-center h-64">
        <span class="material-symbols-outlined animate-spin text-[var(--primary)]" style="font-size: 32px">progress_activity</span>
    </div>
{:else if conversation}
    <div class="grid grid-cols-1 lg:grid-cols-[1fr_300px] gap-6 h-[calc(100vh-140px)]">
        <div class="flex flex-col min-h-0">
            <div class="flex items-center justify-between mb-4">
                <div class="flex items-center gap-3">
                    <a href="/dashboard/conversations" class="p-1.5 rounded-md hover:bg-[var(--surface-container)] text-[var(--on-surface-variant)] transition-all cursor-pointer">
                        <span class="material-symbols-outlined" style="font-size: 20px">arrow_back</span>
                    </a>
                    <h1 class="text-xl font-bold text-[var(--on-surface)]">{conversation.title || 'Conversation'}</h1>
                </div>
            </div>

            <Card padding="none" class="flex-1 overflow-hidden">
                <VirtualList items={conversation.items || []}>
                    {#snippet children(item)}
                        {@const convItem = item as ConversationItem}
                        {@const text = typeof convItem.content === 'string' ? convItem.content : convItem.content?.text || ''}
                        <div class="p-5 {convItem.direction === 'input' ? '' : 'bg-[var(--surface-container-low)]/50'}">
                            <div class="flex gap-3 {convItem.direction === 'input' ? 'flex-row-reverse' : ''}">
                                <div class="w-8 h-8 rounded-full flex items-center justify-center shrink-0 {convItem.direction === 'input' ? 'bg-[var(--surface-container)]' : ''}" style={convItem.direction === 'output' ? 'background: linear-gradient(135deg, var(--primary), var(--primary-container))' : ''}>
                                    <span class="material-symbols-outlined {convItem.direction === 'output' ? 'text-[var(--on-primary)]' : 'text-[var(--on-surface-variant)]'}" style="font-size: 16px">
                                        {convItem.direction === 'output' ? 'bolt' : 'person'}
                                    </span>
                                </div>
                                <div class="flex-1 max-w-[75%]">
                                    <p class="text-xs text-[var(--on-surface-variant)] mb-1.5 uppercase tracking-wider">
                                        {convItem.direction === 'input' ? 'You' : 'HyperInfer'}
                                    </p>
                                    <p class="text-sm text-[var(--on-surface)] whitespace-pre-wrap leading-relaxed">{text}</p>
                                </div>
                            </div>
                        </div>
                    {/snippet}
                </VirtualList>
            </Card>

            <div class="mt-4 flex gap-3">
                <div class="flex-1 relative">
                    <input
                        placeholder="Type a message..."
                        class="w-full px-4 py-3 bg-[var(--surface-container-low)] text-[var(--on-surface)] placeholder:text-[var(--outline)] rounded-xl border-b-2 border-b-[var(--outline-variant)] focus:border-b-[var(--primary)] outline-none transition-colors text-sm"
                    />
                </div>
                <div class="flex gap-2">
                    <button class="p-3 rounded-xl hover:bg-[var(--surface-container)] text-[var(--on-surface-variant)] transition-all cursor-pointer">
                        <span class="material-symbols-outlined" style="font-size: 20px">attachment</span>
                    </button>
                    <button class="p-3 rounded-xl hover:bg-[var(--surface-container)] text-[var(--on-surface-variant)] transition-all cursor-pointer">
                        <span class="material-symbols-outlined" style="font-size: 20px">image</span>
                    </button>
                    <button class="p-3 rounded-xl text-[var(--on-primary)] cursor-pointer" style="background: linear-gradient(135deg, var(--primary), var(--primary-container))">
                        <span class="material-symbols-outlined" style="font-size: 20px">send</span>
                    </button>
                </div>
            </div>
        </div>

        <div class="space-y-4">
            <Card>
                <div class="space-y-4">
                    <h3 class="text-sm font-semibold text-[var(--on-surface)] uppercase tracking-wider">Session Metadata</h3>

                    <div>
                        <p class="text-xs text-[var(--on-surface-variant)] uppercase tracking-wider mb-1">Active Model</p>
                        <div class="flex items-center gap-2">
                            <span class="material-symbols-outlined text-[var(--primary)]" style="font-size: 18px">neurology</span>
                            <span class="text-sm font-medium text-[var(--on-surface)]">HyperInfer-70B</span>
                        </div>
                    </div>

                    <div>
                        <p class="text-xs text-[var(--on-surface-variant)] uppercase tracking-wider mb-1">Status</p>
                        <Badge variant="success" label="Active" />
                    </div>

                    <div>
                        <p class="text-xs text-[var(--on-surface-variant)] uppercase tracking-wider mb-1">Latency</p>
                        <p class="text-sm font-medium text-[var(--on-surface)]">24ms</p>
                    </div>

                    <div>
                        <p class="text-xs text-[var(--on-surface-variant)] uppercase tracking-wider mb-2">Token Usage</p>
                        <div class="flex items-center justify-between text-sm mb-1">
                            <span class="text-[var(--on-surface)] font-medium">84%</span>
                            <span class="text-[var(--on-surface-variant)] text-xs">4.2k / 5k context</span>
                        </div>
                        <div class="h-2 bg-[var(--surface-container)] rounded-full overflow-hidden">
                            <div class="h-full rounded-full" style="width: 84%; background: linear-gradient(90deg, var(--primary), var(--primary-container))"></div>
                        </div>
                    </div>
                </div>
            </Card>

            <div class="space-y-2">
                <Button variant="secondary" size="sm" class="w-full">
                    <span class="material-symbols-outlined" style="font-size: 18px">download</span>
                    Download Transcript
                </Button>
                <Button variant="ghost" size="sm" class="w-full text-[var(--error)]">
                    <span class="material-symbols-outlined" style="font-size: 18px">delete_sweep</span>
                    Clear History
                </Button>
            </div>
        </div>
    </div>
{:else}
    <p class="text-[var(--on-surface-variant)]">Conversation not found</p>
{/if}
