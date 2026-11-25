<script lang="ts">
    import { goto } from '$app/navigation';
    import { page } from '$app/state';
    import { api } from '#lib/api';
    import { auth } from '#lib/stores/auth';
    import type { ApiKey, ModelAlias, Quota, Team, UsageData } from '#lib/types';
    import StatCard from '#lib/components/StatCard.svelte';
    import Card from '#lib/components/Card.svelte';
    import Badge from '#lib/components/Badge.svelte';
    import Breadcrumbs from '#lib/components/Breadcrumbs.svelte';
    import Button from '#lib/components/Button.svelte';
    import Input from '#lib/components/Input.svelte';
    import Modal from '#lib/components/Modal.svelte';

    let team = $state<Team | null>(null);
    let keys = $state<ApiKey[]>([]);
    let usage = $state<UsageData | null>(null);
    let quota = $state<Quota | null>(null);
    let aliases = $state<ModelAlias[]>([]);
    let loading = $state(true);
    let error = $state<string | null>(null);

    // Edit team modal
    let showEditTeam = $state(false);
    let editName = $state('');
    let editBudget = $state('0');
    let editError = $state('');

    // Delete team modal
    let showDeleteTeam = $state(false);
    let deleteTeamError = $state('');

    // Quota modal
    let showQuotaModal = $state(false);
    let rpmLimit = $state('60');
    let tpmLimit = $state('100000');
    let quotaError = $state('');

    // Model alias modal
    let showAliasModal = $state(false);
    let aliasName = $state('');
    let targetModel = $state('');
    let aliasProvider = $state('openai');
    let aliasError = $state('');

    let teamId = $derived(page.params.id);
    let isAdmin = $derived($auth.user?.role === 'admin');
    let isOwnTeam = $derived($auth.user?.team_id === teamId);
    let activeKeys = $derived(keys.filter((k) => k.is_active).length);
    let totalTokens = $derived(usage ? usage.total_input_tokens + usage.total_output_tokens : 0);

    $effect(() => {
        if (!teamId) return;
        let cancelled = false;
        loading = true;
        error = null;
        Promise.all([
            api.getTeam(teamId),
            api.getKeys(teamId).catch(() => []),
            api.getUsage(teamId).catch(() => null),
            api.getQuota(teamId).catch(() => null),
            api.getModelAliases(teamId).catch(() => []),
        ])
            .then(([t, k, u, q, a]) => {
                if (cancelled) return;
                team = t;
                keys = k;
                usage = u;
                quota = q;
                aliases = a;
                loading = false;
            })
            .catch((e) => {
                if (cancelled) return;
                console.error('Failed to load team', e);
                error = 'Failed to load team';
                loading = false;
            });
        return () => {
            cancelled = true;
        };
    });

    function openEditTeam() {
        if (!team) return;
        editName = team.name;
        editBudget = String(team.budget_cents);
        editError = '';
        showEditTeam = true;
    }

    async function saveTeam(e: Event) {
        e.preventDefault();
        if (!team) return;
        editError = '';
        const name = editName.trim();
        const budget = Math.round(Number(editBudget));
        if (!name) {
            editError = 'Team name is required';
            return;
        }
        if (!Number.isFinite(budget) || budget < 0) {
            editError = 'Budget must be a non-negative number';
            return;
        }
        try {
            team = await api.updateTeam(team.id, name, budget);
            showEditTeam = false;
        } catch (err) {
            console.error('Failed to update team', err);
            editError = 'Failed to update team.';
        }
    }

    async function confirmDeleteTeam() {
        if (!team) return;
        deleteTeamError = '';
        try {
            await api.deleteTeam(team.id);
            showDeleteTeam = false;
            goto('/dashboard/teams');
        } catch (err) {
            console.error('Failed to delete team', err);
            deleteTeamError = 'Failed to delete team.';
        }
    }

    function openQuotaModal() {
        rpmLimit = String(quota?.rpm_limit ?? 60);
        tpmLimit = String(quota?.tpm_limit ?? 100000);
        quotaError = '';
        showQuotaModal = true;
    }

    async function saveQuota(e: Event) {
        e.preventDefault();
        if (!team) return;
        quotaError = '';
        const rpm = Math.round(Number(rpmLimit));
        const tpm = Math.round(Number(tpmLimit));
        if (!Number.isFinite(rpm) || rpm <= 0 || !Number.isFinite(tpm) || tpm <= 0) {
            quotaError = 'RPM and TPM limits must be positive integers';
            return;
        }
        try {
            quota = await api.upsertQuota(team.id, rpm, tpm);
            showQuotaModal = false;
        } catch (err) {
            console.error('Failed to save quota', err);
            quotaError = 'Failed to save quota.';
        }
    }

    function openAliasModal() {
        aliasName = '';
        targetModel = '';
        aliasProvider = 'openai';
        aliasError = '';
        showAliasModal = true;
    }

    async function saveAlias(e: Event) {
        e.preventDefault();
        if (!team) return;
        aliasError = '';
        if (!aliasName.trim() || !targetModel.trim()) {
            aliasError = 'Alias and target model are required';
            return;
        }
        try {
            const created = await api.createModelAlias({
                team_id: team.id,
                alias: aliasName.trim(),
                target_model: targetModel.trim(),
                provider: aliasProvider,
            });
            aliases = [created, ...aliases];
            showAliasModal = false;
        } catch (err) {
            console.error('Failed to create model alias', err);
            aliasError = 'Failed to create model alias.';
        }
    }

    async function removeAlias(id: string) {
        try {
            await api.deleteModelAlias(id);
            aliases = aliases.filter((a) => a.id !== id);
        } catch (err) {
            console.error('Failed to delete model alias', err);
        }
    }
</script>

{#if loading}
    <div class="flex items-center justify-center h-64">
        <span class="material-symbols-outlined animate-spin text-[var(--primary)]" style="font-size: 32px">progress_activity</span>
    </div>
{:else if error}
    <div class="flex items-center gap-2 text-[var(--error)]">
        <span class="material-symbols-outlined">error</span>
        {error}
    </div>
{:else if team}
    <div class="space-y-8">
        <Breadcrumbs items={[
            { label: 'Teams', href: '/dashboard/teams' },
            { label: team.name }
        ]} />

        <div class="flex items-center justify-between">
            <div class="flex items-center gap-3">
                <h1 class="text-3xl font-bold text-[var(--on-surface)]">{team.name}</h1>
                <Badge variant="info" label="Admin" />
            </div>
            {#if isAdmin}
                <div class="flex gap-2">
                    <Button variant="secondary" size="sm" onclick={openEditTeam}>
                        <span class="material-symbols-outlined" style="font-size: 18px">edit</span>
                        Edit Team
                    </Button>
                    {#if !isOwnTeam}
                        <Button variant="ghost" size="sm" class="text-[var(--error)]" onclick={() => { deleteTeamError = ''; showDeleteTeam = true; }}>
                            <span class="material-symbols-outlined" style="font-size: 18px">delete</span>
                            Delete Team
                        </Button>
                    {/if}
                </div>
            {/if}
        </div>

        <div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-4">
            <StatCard icon="vpn_key" label="Total Keys" value={String(keys.length)} trend="API keys configured" trendDirection="neutral" />
            <StatCard icon="sensors" label="Active Keys" value={String(activeKeys)} trend={`${activeKeys > 0 ? Math.round((activeKeys / keys.length) * 100) : 0}% active`} trendDirection="up" />
            <StatCard icon="data_usage" label="Usage This Month" value={totalTokens.toLocaleString()} trend="Tokens processed" trendDirection="neutral" />
            <StatCard icon="account_balance_wallet" label="Monthly Budget" value={`$${(team.budget_cents / 100).toFixed(2)}`} trend="Allocated per month" trendDirection="neutral" />
        </div>

        <div class="grid grid-cols-1 lg:grid-cols-2 gap-6">
            <Card>
                <div class="space-y-6">
                    <div>
                        <h2 class="text-lg font-semibold text-[var(--on-surface)]">Monthly Budget</h2>
                        <p class="text-sm text-[var(--on-surface-variant)]">Team: {team.name}</p>
                    </div>

                    <div>
                        <div class="flex items-end justify-between mb-3">
                            <span class="text-3xl font-bold text-[var(--on-surface)]" style="font-family: 'Manrope', sans-serif">
                                {'$'}{(team.budget_cents / 100).toFixed(2)}
                            </span>
                            <span class="text-sm text-[var(--on-surface-variant)]">
                                Monthly budget
                            </span>
                        </div>
                        <div class="flex gap-8 mt-4">
                            <div>
                                <p class="text-xs text-[var(--on-surface-variant)] uppercase tracking-wider">Total Requests</p>
                                <p class="text-sm font-medium text-[var(--on-surface)]">{usage ? usage.total_requests.toLocaleString() : '0'}</p>
                            </div>
                            <div>
                                <p class="text-xs text-[var(--on-surface-variant)] uppercase tracking-wider">Avg Latency</p>
                                <p class="text-sm font-medium text-[var(--on-surface)]">{usage ? `${Math.round(usage.avg_response_time_ms)}ms` : '—'}</p>
                            </div>
                        </div>
                    </div>
                </div>
            </Card>

            <Card>
                <div class="space-y-6">
                    <div class="flex items-center justify-between">
                        <div>
                            <h2 class="text-lg font-semibold text-[var(--on-surface)]">Rate-Limit Quotas</h2>
                            <p class="text-sm text-[var(--on-surface-variant)]">Per-minute request and token ceilings</p>
                        </div>
                        {#if isAdmin}
                            <Button variant="secondary" size="sm" onclick={openQuotaModal}>
                                Configure Quotas
                            </Button>
                        {/if}
                    </div>

                    {#if quota}
                        <div class="grid grid-cols-2 gap-4">
                            <div class="p-4 rounded-lg bg-[var(--surface-container-low)]">
                                <p class="text-xs uppercase tracking-wider text-[var(--on-surface-variant)]">RPM Limit</p>
                                <p class="text-2xl font-bold text-[var(--on-surface)] mt-1">{quota.rpm_limit.toLocaleString()}</p>
                            </div>
                            <div class="p-4 rounded-lg bg-[var(--surface-container-low)]">
                                <p class="text-xs uppercase tracking-wider text-[var(--on-surface-variant)]">TPM Limit</p>
                                <p class="text-2xl font-bold text-[var(--on-surface)] mt-1">{quota.tpm_limit.toLocaleString()}</p>
                            </div>
                        </div>
                    {:else}
                        <p class="text-sm text-[var(--on-surface-variant)]">No custom rate-limit quota configured for this team.</p>
                    {/if}
                </div>
            </Card>
        </div>

        <Card padding="none">
            <div class="p-5 flex items-center justify-between border-b border-[var(--ghost-border)]">
                <div>
                    <h2 class="text-lg font-semibold text-[var(--on-surface)]">Model Aliases</h2>
                    <p class="text-sm text-[var(--on-surface-variant)]">Map friendly model names to provider targets for this team.</p>
                </div>
                {#if isAdmin}
                    <Button size="sm" onclick={openAliasModal}>
                        <span class="material-symbols-outlined" style="font-size: 18px">add</span>
                        Add Alias
                    </Button>
                {/if}
            </div>
            {#if aliases.length === 0}
                <div class="p-8 text-center text-sm text-[var(--on-surface-variant)]">No model aliases configured yet</div>
            {:else}
                <div class="overflow-x-auto">
                    <table class="w-full">
                        <thead>
                            <tr class="bg-[var(--surface-container-low)]">
                                <th class="px-5 py-3 text-left text-xs font-medium uppercase tracking-wider text-[var(--on-surface-variant)]">Alias</th>
                                <th class="px-5 py-3 text-left text-xs font-medium uppercase tracking-wider text-[var(--on-surface-variant)]">Target Model</th>
                                <th class="px-5 py-3 text-left text-xs font-medium uppercase tracking-wider text-[var(--on-surface-variant)]">Provider</th>
                                {#if isAdmin}
                                    <th class="px-5 py-3 text-right text-xs font-medium uppercase tracking-wider text-[var(--on-surface-variant)]">Actions</th>
                                {/if}
                            </tr>
                        </thead>
                        <tbody>
                            {#each aliases as alias, i}
                                <tr class="{i > 0 ? 'border-t border-[var(--ghost-border)]' : ''}">
                                    <td class="px-5 py-3.5 text-sm font-medium text-[var(--on-surface)]">{alias.alias}</td>
                                    <td class="px-5 py-3.5 text-sm font-mono text-[var(--on-surface-variant)]">{alias.target_model}</td>
                                    <td class="px-5 py-3.5 text-sm text-[var(--on-surface-variant)]">{alias.provider}</td>
                                    {#if isAdmin}
                                        <td class="px-5 py-3.5 text-right">
                                            <Button variant="ghost" size="sm" class="text-[var(--error)]" onclick={() => removeAlias(alias.id)}>
                                                Delete
                                            </Button>
                                        </td>
                                    {/if}
                                </tr>
                            {/each}
                        </tbody>
                    </table>
                </div>
            {/if}
        </Card>

        <div class="flex gap-3">
            <Button href="/dashboard/teams/{team.id}/keys" variant="secondary" size="sm">
                <span class="material-symbols-outlined" style="font-size: 18px">vpn_key</span>
                Manage Keys
            </Button>
            <Button href="/dashboard/teams/{team.id}/usage" variant="secondary" size="sm">
                <span class="material-symbols-outlined" style="font-size: 18px">bar_chart</span>
                Usage
            </Button>
            <Button href="/dashboard/teams/{team.id}/budget" variant="secondary" size="sm">
                <span class="material-symbols-outlined" style="font-size: 18px">account_balance_wallet</span>
                Budget
            </Button>
        </div>
    </div>

    <Modal bind:open={showEditTeam}>
        <form class="p-6 space-y-5" onsubmit={saveTeam}>
            <h2 class="text-xl font-semibold text-[var(--on-surface)]">Edit Team</h2>
            <Input id="edit-team-name" label="Team Name" bind:value={editName} required />
            <Input id="edit-team-budget" label="Budget (cents)" type="number" bind:value={editBudget} required />
            {#if editError}
                <div class="text-sm text-[var(--error)]">{editError}</div>
            {/if}
            <div class="flex justify-end gap-2">
                <Button variant="ghost" onclick={() => (showEditTeam = false)}>Cancel</Button>
                <Button type="submit">Save Team</Button>
            </div>
        </form>
    </Modal>

    <Modal bind:open={showDeleteTeam} onclose={() => { deleteTeamError = ''; }}>
        <div class="p-6">
            <h2 class="text-xl font-bold text-[var(--on-surface)] mb-2">Delete Team</h2>
            <p class="text-sm text-[var(--on-surface-variant)] mb-4">
                Are you sure you want to delete <strong>{team.name}</strong>? All associated keys and aliases will be removed.
            </p>
            {#if deleteTeamError}
                <div class="text-sm text-[var(--error)] mb-4">{deleteTeamError}</div>
            {/if}
            <div class="flex justify-end gap-2">
                <Button variant="ghost" onclick={() => (showDeleteTeam = false)}>Cancel</Button>
                <button
                    type="button"
                    class="inline-flex items-center justify-center font-medium rounded-lg px-5 py-2.5 text-sm bg-[var(--error)] text-[var(--on-error)] hover:opacity-90 transition-all cursor-pointer"
                    onclick={confirmDeleteTeam}
                >
                    Delete
                </button>
            </div>
        </div>
    </Modal>

    <Modal bind:open={showQuotaModal}>
        <form class="p-6 space-y-5" onsubmit={saveQuota}>
            <h2 class="text-xl font-semibold text-[var(--on-surface)]">Configure Rate-Limit Quotas</h2>
            <Input id="quota-rpm" label="RPM Limit" type="number" bind:value={rpmLimit} required />
            <Input id="quota-tpm" label="TPM Limit" type="number" bind:value={tpmLimit} required />
            {#if quotaError}
                <div class="text-sm text-[var(--error)]">{quotaError}</div>
            {/if}
            <div class="flex justify-end gap-2">
                <Button variant="ghost" onclick={() => (showQuotaModal = false)}>Cancel</Button>
                <Button type="submit">Save Quotas</Button>
            </div>
        </form>
    </Modal>

    <Modal bind:open={showAliasModal}>
        <form class="p-6 space-y-5" onsubmit={saveAlias}>
            <h2 class="text-xl font-semibold text-[var(--on-surface)]">Add Model Alias</h2>
            <Input id="alias-name" label="Alias" bind:value={aliasName} placeholder="e.g. fast-chat" required />
            <Input id="alias-target" label="Target Model" bind:value={targetModel} placeholder="e.g. gpt-4o-mini" required />
            <div>
                <label for="alias-provider" class="block text-xs font-medium uppercase tracking-wider text-[var(--on-surface-variant)] mb-1.5">
                    Provider
                </label>
                <select
                    id="alias-provider"
                    bind:value={aliasProvider}
                    class="w-full px-4 py-3 bg-[var(--surface-container-low)] text-[var(--on-surface)] rounded-lg border-b-2 border-b-[var(--outline-variant)] focus:border-b-[var(--primary)] outline-none"
                >
                    <option value="openai">OpenAI</option>
                    <option value="anthropic">Anthropic</option>
                    <option value="gemini">Gemini</option>
                    <option value="ollama">Ollama</option>
                </select>
            </div>
            {#if aliasError}
                <div class="text-sm text-[var(--error)]">{aliasError}</div>
            {/if}
            <div class="flex justify-end gap-2">
                <Button variant="ghost" onclick={() => (showAliasModal = false)}>Cancel</Button>
                <Button type="submit">Create Alias</Button>
            </div>
        </form>
    </Modal>
{:else}
    <p class="text-[var(--on-surface-variant)]">Team not found</p>
{/if}
