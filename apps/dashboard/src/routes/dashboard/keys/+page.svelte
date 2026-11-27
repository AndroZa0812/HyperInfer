<script lang="ts">
    import { api } from '#lib/api';
    import { auth } from '#lib/stores/auth';
    import type { ApiKey, CreateApiKeyOptions, CreateApiKeyResponse, UsageData } from '#lib/types';
    import Button from '@hyperinfer/ui/Button.svelte';
    import Input from '@hyperinfer/ui/Input.svelte';
    import Modal from '@hyperinfer/ui/Modal.svelte';
    import Badge from '@hyperinfer/ui/Badge.svelte';
    import StatCard from '#lib/components/StatCard.svelte';
    import Card from '@hyperinfer/ui/Card.svelte';

    let keys = $state<ApiKey[]>([]);
    let loading = $state(true);
    let loadError = $state<string | null>(null);
    let showCreate = $state(false);
    let createError = $state<string | null>(null);
    let newKeyName = $state('');
    let newBudgetCents = $state<string | number>('');
    let newAllowedModels = $state('');
    let newRpmLimit = $state<string | number>('');
    let newTpmLimit = $state<string | number>('');
    let newExpiresAt = $state('');
    let searchQuery = $state('');
    let statusFilter = $state('all');
    let usage = $state<UsageData | null>(null);
    let showSecret = $state(false);
    let createdKey = $state<CreateApiKeyResponse | null>(null);
    let copied = $state(false);

    let filteredKeys = $derived(
        keys.filter(k => {
            const matchesSearch = !searchQuery || (k.name ?? '').toLowerCase().includes(searchQuery.toLowerCase());
            const matchesStatus = statusFilter === 'all' ||
                (statusFilter === 'active' && k.is_active) ||
                (statusFilter === 'revoked' && !k.is_active);
            return matchesSearch && matchesStatus;
        })
    );

    $effect(() => {
        if (!$auth.loading && $auth.user?.team_id) {
            loading = true;
            loadError = null;
            Promise.allSettled([
                api.getKeys($auth.user!.team_id),
                api.getUsage($auth.user!.team_id)
            ])
                .then(([keysResult, usageResult]) => {
                    if (keysResult.status === 'fulfilled') {
                        keys = keysResult.value;
                    } else {
                        console.error('Failed to load keys', keysResult.reason);
                        loadError = 'Failed to load API keys. Please try again.';
                    }
                    if (usageResult.status === 'fulfilled') {
                        usage = usageResult.value;
                    } else {
                        console.error('Failed to load usage summary', usageResult.reason);
                    }
                })
                .finally(() => { loading = false; });
        }
    });

    async function createKey() {
        if (!$auth.user?.team_id) return;
        createError = null;
        const opts: CreateApiKeyOptions = {};
        const budgetStr = String(newBudgetCents ?? '').trim();
        if (budgetStr !== '') {
            const parsed = Math.round(Number(budgetStr));
            if (!Number.isFinite(parsed) || parsed < 0) {
                createError = 'Budget must be a non-negative number of cents.';
                return;
            }
            opts.budget_cents = parsed;
        }
        const models = String(newAllowedModels ?? '')
            .split(',')
            .map((m) => m.trim())
            .filter(Boolean);
        if (models.length > 0) opts.allowed_models = models;
        const rpmStr = String(newRpmLimit ?? '').trim();
        if (rpmStr !== '') {
            const parsed = Math.round(Number(rpmStr));
            if (!Number.isFinite(parsed) || parsed <= 0) {
                createError = 'RPM limit must be a positive number.';
                return;
            }
            opts.rpm_limit = parsed;
        }
        const tpmStr = String(newTpmLimit ?? '').trim();
        if (tpmStr !== '') {
            const parsed = Math.round(Number(tpmStr));
            if (!Number.isFinite(parsed) || parsed <= 0) {
                createError = 'TPM limit must be a positive number.';
                return;
            }
            opts.tpm_limit = parsed;
        }
        const expiresStr = String(newExpiresAt ?? '').trim();
        if (expiresStr !== '') {
            opts.expires_at = new Date(`${expiresStr}T23:59:59Z`).toISOString();
        }
        try {
            const response = await api.createKey($auth.user!.team_id, newKeyName, opts);
            const { key: secret, ...keyMetadata } = response;
            keys = [...keys, keyMetadata];
            showCreate = false;
            newKeyName = '';
            newBudgetCents = '';
            newAllowedModels = '';
            newRpmLimit = '';
            newTpmLimit = '';
            newExpiresAt = '';
            createdKey = { ...keyMetadata, key: secret };
            showSecret = true;
            copied = false;
        } catch (e) {
            console.error('Failed to create key', e);
            createError = e instanceof Error ? e.message : 'Failed to create API key';
        }
    }

    function copyKey() {
        if (!createdKey) return;
        navigator.clipboard?.writeText(createdKey.key).then(() => {
            copied = true;
            setTimeout(() => { copied = false; }, 2000);
        }).catch(() => {});
    }

    function closeSecretModal() {
        showSecret = false;
        createdKey = null;
        copied = false;
    }

    async function revokeKey(keyId: string) {
        try {
            const updated = await api.revokeKey(keyId);
            keys = keys.map(k => k.id === keyId ? updated : k);
        } catch (e) {
            console.error('Failed to revoke key', e);
        }
    }
</script>

<div class="space-y-8">
    <div class="flex items-center justify-between">
        <div>
            <p class="text-xs uppercase tracking-[0.2em] text-[var(--on-surface-variant)] mb-1">Security Infrastructure</p>
            <h1 class="text-3xl font-bold text-[var(--on-surface)]">API Keys</h1>
            <p class="text-sm text-[var(--on-surface-variant)] mt-1">Manage authentication credentials for your production and development clusters.</p>
        </div>
        <Button onclick={() => showCreate = true}>
            <span class="material-symbols-outlined" style="font-size: 18px">add</span>
            Create New Key
        </Button>
    </div>

    <Card padding="none">
        <div class="p-4 flex items-center gap-4">
            <div class="flex-1 relative">
                <span class="material-symbols-outlined absolute left-3 top-1/2 -translate-y-1/2 text-[var(--outline)]" style="font-size: 20px">search</span>
                <input
                    bind:value={searchQuery}
                    placeholder="Search keys..."
                    class="w-full pl-10 pr-4 py-2.5 bg-[var(--surface-container-low)] text-[var(--on-surface)] placeholder:text-[var(--outline)] rounded-lg border-b-2 border-b-[var(--outline-variant)] focus:border-b-[var(--primary)] outline-none transition-colors text-sm"
                />
            </div>
            <div class="flex gap-1 bg-[var(--surface-container-low)] rounded-lg p-1">
                {#each [{l:'All Status',v:'all'},{l:'Active',v:'active'},{l:'Revoked',v:'revoked'}] as tab}
                    <button
                        class="px-3 py-1.5 text-xs font-medium rounded-md transition-all cursor-pointer {statusFilter === tab.v ? 'bg-[var(--surface-container-lowest)] text-[var(--primary)] shadow-sm' : 'text-[var(--on-surface-variant)] hover:text-[var(--on-surface)]'}"
                        onclick={() => statusFilter = tab.v}
                    >{tab.l}</button>
                {/each}
            </div>
        </div>

        {#if loading}
            <div class="flex items-center justify-center h-32">
                <span class="material-symbols-outlined animate-spin text-[var(--primary)]" style="font-size: 24px">progress_activity</span>
            </div>
        {:else if loadError}
            <div class="p-8 text-center text-[var(--error)] text-sm">
                {loadError}
            </div>
        {:else if filteredKeys.length === 0}
            <div class="p-8 text-center text-[var(--on-surface-variant)]">
                {searchQuery || statusFilter !== 'all' ? 'No matching keys found' : 'No API keys yet'}
            </div>
        {:else}
            <div class="overflow-x-auto">
                <table class="w-full">
                    <thead>
                        <tr class="bg-[var(--surface-container-low)]">
                            <th class="px-5 py-3.5 text-left text-xs font-medium uppercase tracking-wider text-[var(--on-surface-variant)]">Key Name</th>
                            <th class="px-5 py-3.5 text-left text-xs font-medium uppercase tracking-wider text-[var(--on-surface-variant)]">Prefix</th>
                            <th class="px-5 py-3.5 text-left text-xs font-medium uppercase tracking-wider text-[var(--on-surface-variant)]">Budget &amp; Scope</th>
                            <th class="px-5 py-3.5 text-left text-xs font-medium uppercase tracking-wider text-[var(--on-surface-variant)]">Created At</th>
                            <th class="px-5 py-3.5 text-left text-xs font-medium uppercase tracking-wider text-[var(--on-surface-variant)]">Status</th>
                            <th class="px-5 py-3.5 text-right text-xs font-medium uppercase tracking-wider text-[var(--on-surface-variant)]">Actions</th>
                        </tr>
                    </thead>
                    <tbody>
                        {#each filteredKeys as key, i}
                            <tr class="{i > 0 ? 'border-t border-[var(--ghost-border)]' : ''} hover:bg-[var(--surface-container-low)]/50 transition-colors">
                                <td class="px-5 py-4">
                                    <div class="flex items-center gap-2">
                                        <span class="material-symbols-outlined text-[var(--primary)]" style="font-size: 18px">vpn_key</span>
                                        <span class="text-sm font-medium text-[var(--on-surface)]">{key.name || 'Unnamed Key'}</span>
                                    </div>
                                </td>
                                <td class="px-5 py-4 text-sm font-mono text-[var(--on-surface-variant)]">{key.prefix ? `${key.prefix}...` : `${key.id.slice(0, 8)}...`}</td>
                                <td class="px-5 py-4 text-xs text-[var(--on-surface-variant)]">
                                    <div class="space-y-0.5">
                                        <div class="font-mono text-[var(--on-surface)]">
                                            {#if key.budget_cents != null}
                                                {'$'}{((key.spent_cents ?? 0) / 100).toFixed(2)} / {'$'}{(key.budget_cents / 100).toFixed(2)}
                                            {:else}
                                                Unlimited
                                            {/if}
                                        </div>
                                        {#if key.allowed_models && key.allowed_models.length > 0}
                                            <div class="truncate max-w-[180px]" title={key.allowed_models.join(', ')}>
                                                Models: {key.allowed_models.join(', ')}
                                            </div>
                                        {/if}
                                        {#if key.rpm_limit || key.tpm_limit}
                                            <div>
                                                {key.rpm_limit ? `${key.rpm_limit} RPM` : ''}{key.rpm_limit && key.tpm_limit ? ' · ' : ''}{key.tpm_limit ? `${key.tpm_limit} TPM` : ''}
                                            </div>
                                        {/if}
                                    </div>
                                </td>
                                <td class="px-5 py-4 text-sm text-[var(--on-surface-variant)]">{new Date(key.created_at).toLocaleDateString()}</td>
                                <td class="px-5 py-4">
                                    <Badge variant={key.is_active ? 'success' : 'error'} label={key.is_active ? 'Active' : 'Revoked'} />
                                </td>
                                <td class="px-5 py-4 text-right">
                                    {#if key.is_active}
                                        <div class="flex items-center justify-end gap-1">
                                            <a href="/dashboard/keys/{key.id}/metrics" class="p-1.5 rounded-md hover:bg-[var(--surface-container)] text-[var(--on-surface-variant)] hover:text-[var(--primary)] transition-all" title="Metrics">
                                                <span class="material-symbols-outlined" style="font-size: 18px">monitoring</span>
                                            </a>
                                            <button class="p-1.5 rounded-md hover:bg-[var(--error)]/10 text-[var(--on-surface-variant)] hover:text-[var(--error)] transition-all cursor-pointer" title="Revoke" onclick={() => revokeKey(key.id)}>
                                                <span class="material-symbols-outlined" style="font-size: 18px">block</span>
                                            </button>
                                        </div>
                                    {:else}
                                        <span class="text-xs text-[var(--outline)]">No Actions Available</span>
                                    {/if}
                                </td>
                            </tr>
                        {/each}
                    </tbody>
                </table>
            </div>
            <div class="px-5 py-3 flex items-center justify-between border-t border-[var(--ghost-border)]">
                <span class="text-xs text-[var(--on-surface-variant)]">Showing 1-{filteredKeys.length} of {keys.length} keys</span>
            </div>
        {/if}
    </Card>

    <div class="grid grid-cols-1 md:grid-cols-3 gap-4">
        <StatCard icon="speed" label="Total Requests" value={usage ? usage.total_requests.toLocaleString() : '0'} />
        <StatCard icon="security" label="Total Tokens" value={usage ? (usage.total_input_tokens + usage.total_output_tokens).toLocaleString() : '0'} />
        <StatCard icon="timer" label="Average Latency" value={usage ? `${Math.round(usage.avg_response_time_ms)}ms` : '0ms'} />
    </div>
</div>

<Modal bind:open={showCreate}>
    <form class="p-6 space-y-6" onsubmit={(e) => { e.preventDefault(); createKey(); }}>
        <div>
            <h2 class="text-xl font-semibold text-[var(--on-surface)]">Create API Key</h2>
            <p class="text-sm text-[var(--on-surface-variant)] mt-1">Generate a new authentication credential.</p>
        </div>
        <Input label="Key Name" bind:value={newKeyName} placeholder="e.g. Production-Main" required />
        <div class="grid grid-cols-2 gap-4">
            <Input label="Budget (cents, optional)" type="number" bind:value={newBudgetCents} placeholder="e.g. 5000" />
            <Input label="Expires On (optional)" type="date" bind:value={newExpiresAt} />
        </div>
        <Input label="Allowed Models (comma-separated, optional)" bind:value={newAllowedModels} placeholder="e.g. gpt-4, gpt-4o-mini" />
        <div class="grid grid-cols-2 gap-4">
            <Input label="RPM Limit (optional)" type="number" bind:value={newRpmLimit} placeholder="e.g. 60" />
            <Input label="TPM Limit (optional)" type="number" bind:value={newTpmLimit} placeholder="e.g. 100000" />
        </div>
        {#if createError}
            <p class="text-sm text-[var(--error)]">{createError}</p>
        {/if}
        <div class="flex gap-3 justify-end">
            <Button variant="ghost" onclick={() => { showCreate = false; createError = null; }}>Cancel</Button>
            <Button type="submit">Create Key</Button>
        </div>
    </form>
</Modal>

<Modal bind:open={showSecret} onclose={closeSecretModal}>
    <div class="p-6 space-y-6">
        <div>
            <h2 class="text-xl font-semibold text-[var(--on-surface)]">API Key Created</h2>
            <p class="text-sm text-[var(--on-surface-variant)] mt-1">Save this key now. It will only be shown once.</p>
        </div>
        <div class="bg-[var(--surface-container-low)] rounded-lg p-4">
            <p class="text-xs text-[var(--on-surface-variant)] mb-2">Secret Key</p>
            <p class="text-sm font-mono text-[var(--on-surface)] break-all">{createdKey?.key}</p>
        </div>
        <div class="flex gap-3 justify-end">
            <Button variant="ghost" onclick={copyKey}>
                <span class="material-symbols-outlined" style="font-size: 18px">{copied ? 'check' : 'content_copy'}</span>
                {copied ? 'Copied!' : 'Copy Key'}
            </Button>
            <Button onclick={closeSecretModal}>Done</Button>
        </div>
    </div>
</Modal>
