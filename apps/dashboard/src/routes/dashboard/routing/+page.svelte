<script lang="ts">
    import { api } from '#lib/api';
    import type { Deployment, CreateDeploymentRequest, ModelSpec, RoutingHealth } from '#lib/types';
    import { onMount } from 'svelte';
    import Card from '@hyperinfer/ui/Card.svelte';
    import Modal from '@hyperinfer/ui/Modal.svelte';
    import Button from '@hyperinfer/ui/Button.svelte';
    import Badge from '@hyperinfer/ui/Badge.svelte';
    import Input from '@hyperinfer/ui/Input.svelte';
    import Select from '@hyperinfer/ui/Select.svelte';

    let deployments = $state<Deployment[]>([]);
    let loading = $state(true);
    let showCreate = $state(false);
    let editingId = $state<string | null>(null);
    let modelFilter = $state('');
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
    <div class="flex flex-wrap items-start justify-between gap-4">
        <div class="min-w-[220px] flex-1">
            <h1 class="text-2xl font-bold text-[var(--on-surface)]">Routing Deployments</h1>
            <p class="text-sm text-[var(--on-surface-variant)] mt-1">Configure upstream provider deployments, routing strategies, and live model catalog sync.</p>
        </div>
        <div class="flex flex-wrap items-center gap-2 w-full sm:w-auto">
            <input
                type="text"
                placeholder="Filter by model..."
                class="h-10 flex-1 sm:w-56 min-w-[160px] px-3.5 bg-[var(--surface-container-low)] text-[var(--on-surface)] placeholder:text-[var(--outline)] rounded-lg border border-[var(--outline-variant)] focus:bg-[var(--surface-container-lowest)] focus:border-[var(--primary)] outline-none text-sm transition-colors duration-200"
                bind:value={modelFilter}
                onkeydown={(e) => e.key === 'Enter' && loadDeployments()}
            />
            <Button
                size="sm"
                class="h-10 shrink-0"
                onclick={() => { showCreate = true; editingId = null; resetForm(); }}
            >
                Add Deployment
            </Button>
        </div>
    </div>

    <Card>
        <div class="flex flex-col xl:flex-row xl:items-center justify-between gap-5">
            <div class="grid grid-cols-1 sm:grid-cols-3 gap-4 sm:gap-6 flex-1 min-w-0">
                <div>
                    <p class="text-xs uppercase tracking-wider text-[var(--on-surface-variant)]">Gateway Status</p>
                    <div class="mt-1.5">
                        <Badge
                            variant={routingHealth?.status === 'healthy' ? 'success' : routingHealth ? 'error' : 'neutral'}
                            label={routingHealth?.status ?? 'unknown'}
                        />
                    </div>
                </div>
                <div>
                    <p class="text-xs uppercase tracking-wider text-[var(--on-surface-variant)]">Active Deployments</p>
                    <p class="text-xl font-bold text-[var(--on-surface)] mt-1">{routingHealth?.active_deployments ?? deployments.filter(d => d.is_active).length}</p>
                </div>
                <div class="min-w-0">
                    <p class="text-xs uppercase tracking-wider text-[var(--on-surface-variant)]">Model Catalog</p>
                    <div class="flex flex-wrap items-center gap-2 mt-1">
                        <span class="text-sm font-semibold text-[var(--on-surface)] whitespace-nowrap">{catalogModels.length} models</span>
                        <Button variant="secondary" size="sm" disabled={syncingCatalog} onclick={syncCatalog}>
                            {syncingCatalog ? 'Syncing...' : 'Sync Catalog'}
                        </Button>
                        {#if catalogStatus}
                            <span class="text-xs text-[var(--on-surface-variant)]">{catalogStatus}</span>
                        {/if}
                    </div>
                </div>
            </div>
            <div class="flex flex-wrap items-end gap-3 pt-4 xl:pt-0 border-t xl:border-t-0 border-[var(--ghost-border)] shrink-0">
                <Select
                    id="routing-strategy"
                    label="Routing Strategy"
                    bind:value={routingStrategy}
                    options={STRATEGIES}
                    class="flex-1 sm:flex-initial min-w-[180px]"
                />
                <Button size="sm" class="h-10 shrink-0" disabled={savingStrategy} onclick={saveStrategy}>
                    {savingStrategy ? 'Saving...' : 'Save Strategy'}
                </Button>
                {#if strategyStatus}
                    <span class="text-xs text-[var(--on-surface-variant)] self-center">{strategyStatus}</span>
                {/if}
            </div>
        </div>
    </Card>

    {#if loading}
        <div class="text-center py-8 text-[var(--on-surface-variant)]">Loading...</div>
    {:else if deployments.length === 0}
        <Card class="text-center py-10">
            <p class="text-sm text-[var(--on-surface-variant)]">
                {#if modelFilter.trim()}
                    No deployments found for model "{modelFilter}"
                {:else}
                    No deployments configured yet. Click <strong>Add Deployment</strong> to register an upstream model.
                {/if}
            </p>
        </Card>
    {:else}
        <Card padding="none">
            <div class="overflow-x-auto">
                <table class="w-full">
                    <thead>
                        <tr class="bg-[var(--surface-container-low)]">
                            <th class="text-left p-3.5 text-xs font-medium uppercase tracking-wider text-[var(--on-surface-variant)]">Name</th>
                            <th class="text-left p-3.5 text-xs font-medium uppercase tracking-wider text-[var(--on-surface-variant)]">Provider</th>
                            <th class="text-left p-3.5 text-xs font-medium uppercase tracking-wider text-[var(--on-surface-variant)]">Model</th>
                            <th class="text-left p-3.5 text-xs font-medium uppercase tracking-wider text-[var(--on-surface-variant)]">Base URL</th>
                            <th class="text-left p-3.5 text-xs font-medium uppercase tracking-wider text-[var(--on-surface-variant)]">Weight</th>
                            <th class="text-left p-3.5 text-xs font-medium uppercase tracking-wider text-[var(--on-surface-variant)]">Status</th>
                            <th class="text-right p-3.5 text-xs font-medium uppercase tracking-wider text-[var(--on-surface-variant)]">Actions</th>
                        </tr>
                    </thead>
                    <tbody>
                        {#each deployments as deployment}
                            <tr class="border-t border-[var(--ghost-border)] hover:bg-[var(--surface-container-low)]/50 transition-colors">
                                <td class="p-3.5 text-sm font-medium text-[var(--on-surface)] whitespace-nowrap">{deployment.name}</td>
                                <td class="p-3.5 text-sm text-[var(--on-surface-variant)] whitespace-nowrap">{deployment.provider}</td>
                                <td class="p-3.5 text-sm font-mono text-[var(--on-surface-variant)] whitespace-nowrap">{deployment.model}</td>
                                <td class="p-3.5 text-sm text-[var(--on-surface-variant)] truncate max-w-[220px]">{deployment.base_url}</td>
                                <td class="p-3.5 text-sm text-[var(--on-surface-variant)]">{deployment.weight}</td>
                                <td class="p-3.5">
                                    <Badge variant={deployment.is_active ? 'success' : 'error'} label={deployment.is_active ? 'Active' : 'Inactive'} />
                                </td>
                                <td class="p-3.5 text-right">
                                    <div class="flex justify-end gap-2">
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
        <div class="p-6 max-h-[85vh] overflow-y-auto">
            <h2 class="text-xl font-bold text-[var(--on-surface)] mb-4">{editingId ? 'Edit Deployment' : 'Create Deployment'}</h2>
            {#if createError}
                <div class="text-[var(--error)] text-sm mb-4">{createError}</div>
            {/if}
            <div class="space-y-4">
                <Input id="dep-name" label="Name" bind:value={formData.name} />
                <Select
                    id="dep-provider"
                    label="Provider"
                    bind:value={formData.provider}
                    options={[
                        { value: 'openai', label: 'OpenAI' },
                        { value: 'anthropic', label: 'Anthropic' },
                        { value: 'gemini', label: 'Google Gemini' },
                    ]}
                />
                <Input
                    id="dep-model"
                    label="Model"
                    suggestions={providerCatalogModels.map((m) => ({ value: m.id, label: m.name }))}
                    bind:value={formData.model}
                />
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
                <div class="grid grid-cols-1 sm:grid-cols-2 gap-4">
                    <div class="space-y-1.5">
                        <label for="dep-weight" class="block text-xs font-medium uppercase tracking-wider text-[var(--on-surface-variant)]">Weight</label>
                        <input
                            id="dep-weight"
                            type="number"
                            class="h-10 w-full px-3.5 bg-[var(--surface-container-low)] text-[var(--on-surface)] rounded-lg border border-[var(--outline-variant)] focus:bg-[var(--surface-container-lowest)] focus:border-[var(--primary)] outline-none text-sm transition-colors duration-200"
                            bind:value={formData.weight}
                            min="0"
                        />
                    </div>
                    <div class="space-y-1.5">
                        <label for="dep-priority" class="block text-xs font-medium uppercase tracking-wider text-[var(--on-surface-variant)]">Priority</label>
                        <input
                            id="dep-priority"
                            type="number"
                            class="h-10 w-full px-3.5 bg-[var(--surface-container-low)] text-[var(--on-surface)] rounded-lg border border-[var(--outline-variant)] focus:bg-[var(--surface-container-lowest)] focus:border-[var(--primary)] outline-none text-sm transition-colors duration-200"
                            bind:value={formData.priority}
                            min="0"
                        />
                    </div>
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