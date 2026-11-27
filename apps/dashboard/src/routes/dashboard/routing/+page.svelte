<script lang="ts">
    import { api } from '#lib/api';
    import type { Deployment, CreateDeploymentRequest, ModelSpec, RoutingHealth } from '#lib/types';
    import { onMount } from 'svelte';
    import Card from '@hyperinfer/ui/Card.svelte';
    import Modal from '@hyperinfer/ui/Modal.svelte';
    import Button from '@hyperinfer/ui/Button.svelte';
    import Badge from '@hyperinfer/ui/Badge.svelte';
    import Input from '@hyperinfer/ui/Input.svelte';

    let deployments = $state<Deployment[]>([]);
    let loading = $state(true);
    let showCreate = $state(false);
    let editingId = $state<string | null>(null);
    let modelFilter = $state('gpt-4');
    let createError = $state('');
    let pendingDeleteId = $state<string | null>(null);
    let showDeleteConfirm = $state(false);
    let deleteError = $state('');

    let routingStrategy = $state('weighted-shuffle');
    let routingHealth = $state<RoutingHealth | null>(null);
    let savingStrategy = $state(false);
    let strategyStatus = $state('');
    let modelNameOverride = $state('');
    let catalogModels = $state<ModelSpec[]>([]);
    let catalogSource = $state('');
    let syncingCatalog = $state(false);
    let catalogStatus = $state('');

    let providerCatalogModels = $derived(
        catalogModels.filter((m) => m.provider === formData.provider)
    );
    let matchedCatalogModel = $derived(
        providerCatalogModels.find((m) => m.id === formData.model.trim()) ??
        catalogModels.find((m) => m.id === formData.model.trim()) ??
        null
    );

    const STRATEGIES = [
        { value: 'weighted-shuffle', label: 'Weighted Shuffle' },
        { value: 'round-robin', label: 'Round Robin' },
        { value: 'least-busy', label: 'Least Busy' },
        { value: 'latency-based', label: 'Latency Based' },
        { value: 'cost-based', label: 'Cost Based' },
    ];

    let formData = $state<CreateDeploymentRequest>({
        name: '',
        provider: 'openai',
        model: 'gpt-4',
        api_key_ref: '',
        base_url: 'https://api.openai.com/v1',
        is_active: true,
        weight: 1,
        priority: 0,
    });

    onMount(async () => {
        await Promise.all([loadDeployments(), loadRoutingStatus(), loadCatalog()]);
    });

    async function loadCatalog() {
        try {
            const res = await api.getCatalog();
            catalogModels = res.models;
            catalogSource = res.source;
        } catch {
            // Optional live catalog
        }
    }

    async function syncCatalog() {
        syncingCatalog = true;
        catalogStatus = '';
        try {
            const res = await api.refreshCatalog();
            catalogStatus = `Synced ${res.count} models`;
            await loadCatalog();
        } catch (e) {
            console.error('Failed to sync catalog', e);
            catalogStatus = 'Sync failed';
        } finally {
            syncingCatalog = false;
        }
    }

    async function loadRoutingStatus() {
        const [configRes, healthRes] = await Promise.allSettled([
            api.getRoutingConfig(),
            api.getRoutingHealth(),
        ]);
        if (configRes.status === 'fulfilled') {
            routingStrategy = configRes.value.strategy;
        }
        if (healthRes.status === 'fulfilled') {
            routingHealth = healthRes.value;
            if (configRes.status !== 'fulfilled') {
                routingStrategy = healthRes.value.strategy;
            }
        }
    }

    async function saveStrategy() {
        savingStrategy = true;
        strategyStatus = '';
        try {
            const updated = await api.updateRoutingConfig(routingStrategy);
            routingStrategy = updated.strategy;
            strategyStatus = 'Strategy saved';
            await loadRoutingStatus();
        } catch (e) {
            console.error('Failed to update routing strategy', e);
            strategyStatus = 'Failed to save strategy';
        } finally {
            savingStrategy = false;
        }
    }

    async function loadDeployments() {
        loading = true;
        try {
            deployments = await api.getDeployments(modelFilter);
        } catch (e) {
            console.error('Failed to load deployments', e);
        } finally {
            loading = false;
        }
    }

    function buildPayload(): CreateDeploymentRequest {
        const metadata: Record<string, unknown> = { ...(formData.metadata || {}) };
        if (modelNameOverride.trim()) {
            metadata.model_name_override = modelNameOverride.trim();
        } else {
            delete metadata.model_name_override;
        }
        return { ...formData, metadata };
    }

    async function createDeployment() {
        createError = '';
        if (!formData.name.trim()) {
            createError = 'Name is required';
            return;
        }
        if (!formData.base_url.trim()) {
            createError = 'Base URL is required';
            return;
        }
        try {
            const deployment = await api.createDeployment(buildPayload());
            deployments = [...deployments, deployment];
            showCreate = false;
            resetForm();
            await loadRoutingStatus();
        } catch (e) {
            console.error('Failed to create deployment', e);
            createError = 'Failed to create deployment';
        }
    }

    async function updateDeployment() {
        if (!editingId) return;
        createError = '';
        try {
            const deployment = await api.updateDeployment(editingId, buildPayload());
            deployments = deployments.map(d => d.id === editingId ? deployment : d);
            editingId = null;
            showCreate = false;
            resetForm();
            await loadRoutingStatus();
        } catch (e) {
            console.error('Failed to update deployment', e);
            createError = 'Failed to update deployment';
        }
    }

    function confirmDelete(id: string) {
        pendingDeleteId = id;
        deleteError = '';
        showDeleteConfirm = true;
    }

    function cancelDelete() {
        pendingDeleteId = null;
        deleteError = '';
        showDeleteConfirm = false;
    }

    async function deleteDeployment() {
        if (!pendingDeleteId) return;
        deleteError = '';
        try {
            await api.deleteDeployment(pendingDeleteId);
            deployments = deployments.filter(d => d.id !== pendingDeleteId);
            pendingDeleteId = null;
            showDeleteConfirm = false;
        } catch (e) {
            console.error('Failed to delete deployment', e);
            deleteError = 'Failed to delete deployment. Please try again.';
        }
    }

    function startEdit(deployment: Deployment) {
        editingId = deployment.id;
        const meta = deployment.metadata as Record<string, unknown> | undefined;
        modelNameOverride = typeof meta?.model_name_override === 'string' ? meta.model_name_override : '';
        formData = {
            name: deployment.name,
            provider: deployment.provider,
            model: deployment.model,
            api_key_ref: deployment.api_key_ref || '',
            base_url: deployment.base_url,
            is_active: deployment.is_active,
            weight: deployment.weight,
            priority: deployment.priority,
            max_tpm: deployment.max_tpm,
            max_rpm: deployment.max_rpm,
            cost_per_1k_input_tokens: deployment.cost_per_1k_input_tokens,
            cost_per_1k_output_tokens: deployment.cost_per_1k_output_tokens,
            metadata: deployment.metadata,
            sort_order: deployment.sort_order,
        };
        showCreate = true;
    }

    function resetForm() {
        modelNameOverride = '';
        formData = {
            name: '',
            provider: 'openai',
            model: 'gpt-4',
            api_key_ref: '',
            base_url: 'https://api.openai.com/v1',
            is_active: true,
            weight: 1,
            priority: 0,
        };
    }

    function cancelEdit() {
        editingId = null;
        showCreate = false;
        resetForm();
    }
</script>

<div class="space-y-6">
    <div class="flex items-center justify-between">
        <h1 class="text-2xl font-bold text-[var(--on-surface)]">Routing Deployments</h1>
        <div class="flex gap-2">
            <input
                type="text"
                placeholder="Filter by model..."
                class="px-3 py-2 bg-[var(--surface-container-low)] text-[var(--on-surface)] placeholder:text-[var(--outline)] rounded-lg border-b-2 border-b-[var(--outline-variant)] focus:border-b-[var(--primary)] outline-none transition-colors"
                bind:value={modelFilter}
                onkeydown={(e) => e.key === 'Enter' && loadDeployments()}
            />
            <Button
                onclick={() => { showCreate = true; editingId = null; resetForm(); }}
            >
                Add Deployment
            </Button>
        </div>
    </div>

    <Card>
        <div class="flex flex-wrap items-center justify-between gap-4">
            <div class="flex items-center gap-6">
                <div>
                    <p class="text-xs uppercase tracking-wider text-[var(--on-surface-variant)]">Gateway Status</p>
                    <div class="mt-1">
                        <Badge
                            variant={routingHealth?.status === 'healthy' ? 'success' : routingHealth ? 'error' : 'neutral'}
                            label={routingHealth?.status ?? 'unknown'}
                        />
                    </div>
                </div>
                <div>
                    <p class="text-xs uppercase tracking-wider text-[var(--on-surface-variant)]">Active Deployments</p>
                    <p class="text-xl font-bold text-[var(--on-surface)] mt-0.5">{routingHealth?.active_deployments ?? deployments.filter(d => d.is_active).length}</p>
                </div>
                <div>
                    <p class="text-xs uppercase tracking-wider text-[var(--on-surface-variant)]">Model Catalog</p>
                    <div class="flex items-center gap-2 mt-0.5">
                        <span class="text-sm font-semibold text-[var(--on-surface)]">{catalogModels.length} models</span>
                        <Button variant="ghost" size="sm" disabled={syncingCatalog} onclick={syncCatalog}>
                            {syncingCatalog ? 'Syncing...' : 'Sync Catalog'}
                        </Button>
                        {#if catalogStatus}
                            <span class="text-xs text-[var(--on-surface-variant)]">{catalogStatus}</span>
                        {/if}
                    </div>
                </div>
            </div>
            <div class="flex items-end gap-3">
                <div>
                    <label for="routing-strategy" class="block text-xs font-medium uppercase tracking-wider text-[var(--on-surface-variant)] mb-1">
                        Routing Strategy
                    </label>
                    <select
                        id="routing-strategy"
                        bind:value={routingStrategy}
                        class="px-3 py-2 bg-[var(--surface-container-low)] text-[var(--on-surface)] rounded-lg border-b-2 border-b-[var(--outline-variant)] focus:border-b-[var(--primary)] outline-none text-sm"
                    >
                        {#each STRATEGIES as s}
                            <option value={s.value}>{s.label}</option>
                        {/each}
                    </select>
                </div>
                <Button size="sm" disabled={savingStrategy} onclick={saveStrategy}>
                    {savingStrategy ? 'Saving...' : 'Save Strategy'}
                </Button>
                {#if strategyStatus}
                    <span class="text-xs text-[var(--on-surface-variant)]">{strategyStatus}</span>
                {/if}
            </div>
        </div>
    </Card>

    {#if loading}
        <div class="text-center py-8 text-[var(--on-surface-variant)]">Loading...</div>
    {:else if deployments.length === 0}
        <div class="text-center py-8 text-[var(--on-surface-variant)]">No deployments found for model "{modelFilter}"</div>
    {:else}
        <Card variant="outlined" padding="none">
            <div class="overflow-x-auto">
                <table class="w-full">
                    <thead>
                        <tr class="bg-[var(--surface-container-low)]">
                            <th class="text-left p-3 text-xs font-medium uppercase tracking-wider text-[var(--on-surface-variant)]">Name</th>
                            <th class="text-left p-3 text-xs font-medium uppercase tracking-wider text-[var(--on-surface-variant)]">Provider</th>
                            <th class="text-left p-3 text-xs font-medium uppercase tracking-wider text-[var(--on-surface-variant)]">Model</th>
                            <th class="text-left p-3 text-xs font-medium uppercase tracking-wider text-[var(--on-surface-variant)]">Base URL</th>
                            <th class="text-left p-3 text-xs font-medium uppercase tracking-wider text-[var(--on-surface-variant)]">Weight</th>
                            <th class="text-left p-3 text-xs font-medium uppercase tracking-wider text-[var(--on-surface-variant)]">Status</th>
                            <th class="text-left p-3 text-xs font-medium uppercase tracking-wider text-[var(--on-surface-variant)]">Actions</th>
                        </tr>
                    </thead>
                    <tbody>
                        {#each deployments as deployment}
                            <tr class="border-t border-[var(--ghost-border)] hover:bg-[var(--surface-container-low)]/50 transition-colors">
                                <td class="p-3 text-sm text-[var(--on-surface)]">{deployment.name}</td>
                                <td class="p-3 text-sm text-[var(--on-surface-variant)]">{deployment.provider}</td>
                                <td class="p-3 text-sm text-[var(--on-surface-variant)]">{deployment.model}</td>
                                <td class="p-3 text-sm text-[var(--on-surface-variant)] truncate max-w-[200px]">{deployment.base_url}</td>
                                <td class="p-3 text-sm text-[var(--on-surface-variant)]">{deployment.weight}</td>
                                <td class="p-3">
                                    <Badge variant={deployment.is_active ? 'success' : 'error'} label={deployment.is_active ? 'Active' : 'Inactive'} />
                                </td>
                                <td class="p-3">
                                    <div class="flex gap-2">
                                        <Button variant="secondary" size="sm" onclick={() => startEdit(deployment)}>Edit</Button>
                                        <Button variant="ghost" size="sm" class="text-[var(--error)]" onclick={() => confirmDelete(deployment.id)}>Delete</Button>
                                    </div>
                                </td>
                            </tr>
                        {/each}
                    </tbody>
                </table>
            </div>
        </Card>
    {/if}

    <Modal bind:open={showCreate}>
        <div class="p-6">
            <h2 class="text-xl font-bold text-[var(--on-surface)] mb-4">{editingId ? 'Edit Deployment' : 'Create Deployment'}</h2>
            {#if createError}
                <div class="text-[var(--error)] text-sm mb-4">{createError}</div>
            {/if}
            <div class="space-y-4">
                <Input id="dep-name" label="Name" bind:value={formData.name} />
                <div>
                    <label for="dep-provider" class="block text-xs font-medium uppercase tracking-wider text-[var(--on-surface-variant)] mb-1.5">Provider</label>
                    <select
                        id="dep-provider"
                        class="w-full px-4 py-3 bg-[var(--surface-container-low)] text-[var(--on-surface)] rounded-lg border-b-2 border-b-[var(--outline-variant)] focus:border-b-[var(--primary)] outline-none transition-colors"
                        bind:value={formData.provider}
                    >
                        <option value="openai">OpenAI</option>
                        <option value="anthropic">Anthropic</option>
                        <option value="gemini">Google Gemini</option>
                    </select>
                </div>
                <Input id="dep-model" label="Model" list="routing-catalog-models" bind:value={formData.model} />
                <datalist id="routing-catalog-models">
                    {#each providerCatalogModels as m}
                        <option value={m.id}>{m.name}</option>
                    {/each}
                </datalist>
                {#if matchedCatalogModel}
                    <p class="text-xs text-[var(--on-surface-variant)] -mt-2">
                        Catalog ({catalogSource || matchedCatalogModel.source}):
                        {#if matchedCatalogModel.context_window}
                            {matchedCatalogModel.context_window.toLocaleString()} ctx
                        {/if}
                        {#if matchedCatalogModel.pricing}
                            · {'$'}{matchedCatalogModel.pricing.input_per_1m_usd}/M in · {'$'}{matchedCatalogModel.pricing.output_per_1m_usd}/M out
                        {/if}
                    </p>
                {/if}
                <Input id="dep-model-override" label="Model Name Override (optional)" bind:value={modelNameOverride} placeholder="e.g. gpt-4o-2024-08-06" />
                <Input id="dep-base-url" label="Base URL" bind:value={formData.base_url} />
                <Input id="dep-api-key-ref" label="API Key Ref" bind:value={formData.api_key_ref} placeholder="env:OPENAI_API_KEY" />
                <div>
                    <label for="dep-weight" class="block text-xs font-medium uppercase tracking-wider text-[var(--on-surface-variant)] mb-1.5">Weight</label>
                    <input
                        id="dep-weight"
                        type="number"
                        class="w-full px-4 py-3 bg-[var(--surface-container-low)] text-[var(--on-surface)] rounded-lg border-b-2 border-b-[var(--outline-variant)] focus:border-b-[var(--primary)] outline-none transition-colors"
                        bind:value={formData.weight}
                        min="0"
                    />
                </div>
                <div>
                    <label for="dep-priority" class="block text-xs font-medium uppercase tracking-wider text-[var(--on-surface-variant)] mb-1.5">Priority</label>
                    <input
                        id="dep-priority"
                        type="number"
                        class="w-full px-4 py-3 bg-[var(--surface-container-low)] text-[var(--on-surface)] rounded-lg border-b-2 border-b-[var(--outline-variant)] focus:border-b-[var(--primary)] outline-none transition-colors"
                        bind:value={formData.priority}
                        min="0"
                    />
                </div>
                <div class="flex items-center gap-2">
                    <input type="checkbox" id="dep-is-active" bind:checked={formData.is_active} class="rounded border-[var(--outline-variant)]" />
                    <label for="dep-is-active" class="text-sm text-[var(--on-surface)]">Active</label>
                </div>
            </div>
            <div class="flex justify-end gap-2 mt-6">
                <Button variant="ghost" onclick={cancelEdit}>Cancel</Button>
                <Button onclick={editingId ? updateDeployment : createDeployment}>
                    {editingId ? 'Update' : 'Create'}
                </Button>
            </div>
        </div>
    </Modal>

    <Modal bind:open={showDeleteConfirm} onclose={cancelDelete}>
        <div class="p-6">
            <h2 class="text-xl font-bold text-[var(--on-surface)] mb-2">Delete Deployment</h2>
            <p class="text-sm text-[var(--on-surface-variant)] mb-4">Are you sure you want to delete this deployment? This action cannot be undone.</p>
            {#if deleteError}
                <div class="text-[var(--error)] text-sm mb-4">{deleteError}</div>
            {/if}
            <div class="flex justify-end gap-2">
                <Button variant="ghost" onclick={cancelDelete}>Cancel</Button>
                <button
                    type="button"
                    class="inline-flex items-center justify-center font-medium rounded-lg px-5 py-2.5 text-sm bg-[var(--error)] text-[var(--on-error)] hover:opacity-90 transition-all cursor-pointer"
                    onclick={deleteDeployment}
                >
                    Delete
                </button>
            </div>
        </div>
    </Modal>
</div>