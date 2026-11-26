<script lang="ts">
    import { auth } from '#lib/stores/auth';
    import { theme } from '#lib/stores/theme';
    import { api } from '#lib/api';
    import type { RedactionConfig, SemanticCacheConfig } from '#lib/types';
    import Card from '#lib/components/Card.svelte';
    import Button from '#lib/components/Button.svelte';
    import Input from '#lib/components/Input.svelte';
    import Badge from '#lib/components/Badge.svelte';
    import TabBar from '#lib/components/TabBar.svelte';

    let currentPassword = $state('');
    let newPassword = $state('');
    let confirmPassword = $state('');
    let saving = $state(false);
    let success = $state('');
    let error = $state('');
    let activeTab = $state('profile');

    // Gateway Plugins, Redaction & Semantic Cache state (HYP-73)
    let pipelinePlugins = $state<string[]>([]);
    let redaction = $state<RedactionConfig>({
        redact_api_keys: true,
        redact_bearer_tokens: true,
        redact_emails: true,
        redact_ssns: true,
        custom_patterns: [],
        redact_requests: true,
        redact_responses: true,
        reversible: false,
        placeholder_format: 'indexed',
        smart: {
            enabled: false,
            backend: 'presidio',
            presidio_url: 'http://127.0.0.1:5001',
            presidio_language: 'en',
            presidio_entities: [
                'PERSON',
                'LOCATION',
                'PHONE_NUMBER',
                'CREDIT_CARD',
                'IBAN_CODE',
                'US_SSN',
                'EMAIL_ADDRESS',
            ],
            presidio_score_threshold: 0.5,
            llm_base_url: 'http://127.0.0.1:11434/v1',
            llm_model: 'qwen2.5:3b',
            llm_api_key: '',
            llm_categories: ['pii', 'secrets', 'financial', 'internal_names'],
            timeout_ms: 3000,
            fail_mode: 'fallback_regex',
        },
    });
    let customPatternsText = $state('');
    let presidioEntitiesText = $state(
        'PERSON, LOCATION, PHONE_NUMBER, CREDIT_CARD, IBAN_CODE, US_SSN, EMAIL_ADDRESS'
    );
    let presidioScoreText = $state('0.5');
    let llmCategoriesText = $state('pii, secrets, financial, internal_names');
    let smartTimeoutText = $state<string | number>('3000');
    let redactionStatus = $state('');
    let redactionError = $state('');
    let savingRedaction = $state(false);

    let testSample = $state('');
    let testOutput = $state('');
    let testVaultEntries = $state<Record<string, string> | null>(null);
    let testError = $state('');

    let cache = $state<SemanticCacheConfig>({
        enabled: false,
        similarity_threshold: 0.92,
        ttl_secs: 3600,
        embedding_provider: 'openai',
        embedding_model: 'text-embedding-3-small',
        sse_replay: true,
    });
    let cacheThresholdText = $state('0.92');
    let cacheTtlText = $state<string | number>('3600');
    let cacheStatus = $state('');
    let cacheError = $state('');
    let savingCache = $state(false);
    let gatewayLoading = $state(true);
    let gatewayLoadError = $state('');
    let gatewayLoaded = false;

    let isAdmin = $derived($auth.user?.role === 'admin');
    let tabs = $derived([
        { label: 'Profile', value: 'profile' },
        { label: 'Preferences', value: 'preferences' },
        { label: 'Security', value: 'security' },
        ...(isAdmin ? [{ label: 'Plugins & Cache', value: 'gateway' }] : []),
    ]);

    $effect(() => {
        if (activeTab === 'gateway' && isAdmin && !gatewayLoaded) {
            gatewayLoaded = true;
            loadGatewaySettings();
        }
    });

    async function loadGatewaySettings() {
        gatewayLoading = true;
        gatewayLoadError = '';
        try {
            const settings = await api.getGatewaySettings();
            pipelinePlugins = settings.pipeline_plugins;
            redaction = {
                ...redaction,
                ...settings.redaction,
                smart: {
                    ...redaction.smart,
                    ...(settings.redaction.smart ?? {}),
                    llm_api_key: settings.redaction.smart?.llm_api_key ?? '',
                },
            };
            customPatternsText = settings.redaction.custom_patterns
                .map(([pat, rep]) => `${pat} => ${rep}`)
                .join('\n');
            presidioEntitiesText = (redaction.smart.presidio_entities ?? []).join(', ');
            presidioScoreText = String(redaction.smart.presidio_score_threshold ?? 0.5);
            llmCategoriesText = (redaction.smart.llm_categories ?? []).join(', ');
            smartTimeoutText = String(redaction.smart.timeout_ms ?? 3000);
            cache = settings.cache;
            cacheThresholdText = String(settings.cache.similarity_threshold);
            cacheTtlText = String(settings.cache.ttl_secs);
        } catch (e) {
            console.error('Failed to load gateway settings', e);
            gatewayLoadError = 'Failed to load gateway settings.';
        } finally {
            gatewayLoading = false;
        }
    }

    async function saveRedaction() {
        savingRedaction = true;
        redactionStatus = '';
        redactionError = '';
        const customPatterns: [string, string][] = customPatternsText
            .split('\n')
            .map((line) => line.trim())
            .filter(Boolean)
            .map((line) => {
                const idx = line.indexOf('=>');
                if (idx === -1) return [line, '[REDACTED]'] as [string, string];
                return [line.slice(0, idx).trim(), line.slice(idx + 2).trim() || '[REDACTED]'] as [string, string];
            });
        const presidioScore = Number(presidioScoreText);
        const timeoutMs = Math.round(Number(smartTimeoutText));
        if (redaction.smart.enabled) {
            if (!Number.isFinite(timeoutMs) || timeoutMs <= 0) {
                redactionError = 'Smart redaction timeout must be a positive number of milliseconds';
                savingRedaction = false;
                return;
            }
            if (
                redaction.smart.backend === 'presidio' &&
                (!Number.isFinite(presidioScore) || presidioScore < 0 || presidioScore > 1)
            ) {
                redactionError = 'Presidio score threshold must be between 0.0 and 1.0';
                savingRedaction = false;
                return;
            }
        }
        const presidioEntities = presidioEntitiesText
            .split(',')
            .map((s) => s.trim())
            .filter(Boolean);
        const llmCategories = llmCategoriesText
            .split(',')
            .map((s) => s.trim())
            .filter(Boolean);
        const llmApiKey = redaction.smart.llm_api_key?.trim() || undefined;
        try {
            const updated = await api.updateRedactionConfig({
                ...redaction,
                custom_patterns: customPatterns,
                smart: {
                    ...redaction.smart,
                    presidio_entities: presidioEntities,
                    presidio_score_threshold: Number.isFinite(presidioScore) ? presidioScore : 0.5,
                    llm_categories: llmCategories,
                    llm_api_key: llmApiKey,
                    timeout_ms: Number.isFinite(timeoutMs) && timeoutMs > 0 ? timeoutMs : 3000,
                },
            });
            redaction = {
                ...updated,
                smart: {
                    ...updated.smart,
                    llm_api_key: updated.smart.llm_api_key ?? '',
                },
            };
            presidioEntitiesText = (redaction.smart.presidio_entities ?? []).join(', ');
            presidioScoreText = String(redaction.smart.presidio_score_threshold ?? 0.5);
            llmCategoriesText = (redaction.smart.llm_categories ?? []).join(', ');
            smartTimeoutText = String(redaction.smart.timeout_ms ?? 3000);
            redactionStatus = 'Redaction settings saved';
        } catch (e) {
            console.error('Failed to save redaction settings', e);
            redactionError = 'Invalid regex pattern or smart redaction configuration';
        } finally {
            savingRedaction = false;
        }
    }

    async function runRedactionTest() {
        if (!testSample.trim()) return;
        testError = '';
        testVaultEntries = null;
        try {
            const res = await api.testRedaction(testSample);
            testOutput = res.redacted;
            testVaultEntries =
                res.vault_entries && Object.keys(res.vault_entries).length > 0
                    ? res.vault_entries
                    : null;
        } catch (e) {
            console.error('Failed to test redaction', e);
            testOutput = '';
            testError = 'Redaction test failed (check smart redaction connector status or fail-closed mode).';
        }
    }

    async function saveCache() {
        savingCache = true;
        cacheStatus = '';
        cacheError = '';
        const threshold = Number(cacheThresholdText);
        const ttl = Math.round(Number(cacheTtlText));
        if (!Number.isFinite(threshold) || threshold < 0.5 || threshold > 1.0) {
            cacheError = 'Similarity threshold must be between 0.50 and 1.00';
            savingCache = false;
            return;
        }
        if (!Number.isFinite(ttl) || ttl <= 0) {
            cacheError = 'TTL must be a positive number of seconds';
            savingCache = false;
            return;
        }
        try {
            const updated = await api.updateCacheConfig({
                ...cache,
                similarity_threshold: threshold,
                ttl_secs: ttl,
            });
            cache = updated;
            cacheThresholdText = String(updated.similarity_threshold);
            cacheTtlText = String(updated.ttl_secs);
            cacheStatus = 'Cache settings saved';
        } catch (e) {
            console.error('Failed to save cache settings', e);
            cacheError = 'Failed to save cache settings';
        } finally {
            savingCache = false;
        }
    }

    async function handleChangePassword(e: Event) {
        e.preventDefault();
        error = '';
        success = '';

        if (!currentPassword) { error = 'Current password is required'; return; }
        if (newPassword.length < 8) { error = 'New password must be at least 8 characters'; return; }
        if (newPassword !== confirmPassword) { error = 'Passwords do not match'; return; }

        saving = true;
        try {
            await api.changePassword(currentPassword, newPassword);
            success = 'Password updated successfully';
            currentPassword = '';
            newPassword = '';
            confirmPassword = '';
        } catch (e: any) {
            error = e.message?.includes('401') ? 'Current password is incorrect' : 'Failed to update password';
        } finally {
            saving = false;
        }
    }
</script>

<div class="space-y-8 max-w-4xl">
    <div>
        <h1 class="text-3xl font-bold text-[var(--on-surface)]">Settings</h1>
        <p class="text-sm text-[var(--on-surface-variant)] mt-1">Manage your account settings and technical preferences.</p>
    </div>

    <TabBar items={tabs} bind:active={activeTab} />

    {#if activeTab === 'profile'}
        <Card>
            <div class="space-y-6">
                <h2 class="text-lg font-semibold text-[var(--on-surface)]">Account Overview</h2>
                <p class="text-sm text-[var(--on-surface-variant)]">Your account identity and team membership.</p>

                <div class="flex items-center gap-4">
                    <div class="w-16 h-16 rounded-full flex items-center justify-center text-xl font-bold text-[var(--on-primary)]" style="background: linear-gradient(135deg, var(--primary), var(--primary-container))">
                        {$auth.user?.email?.[0]?.toUpperCase() || '?'}
                    </div>
                    <div>
                        <p class="text-sm font-medium text-[var(--on-surface)]">{$auth.user?.email}</p>
                        <p class="text-xs text-[var(--on-surface-variant)]">Enterprise identity verified</p>
                    </div>
                </div>

                <Input label="Email Address" value={$auth.user?.email || ''} disabled />

                <div class="grid grid-cols-2 gap-4">
                    <div>
                        <p class="text-xs font-medium uppercase tracking-wider text-[var(--on-surface-variant)] mb-1.5">Role</p>
                        <Badge variant="info" label={$auth.user?.role || 'member'} />
                    </div>
                    <div>
                        <p class="text-xs font-medium uppercase tracking-wider text-[var(--on-surface-variant)] mb-1.5">Team ID</p>
                        <p class="text-sm font-medium text-[var(--on-surface)] font-mono">{$auth.user?.team_id || '—'}</p>
                    </div>
                </div>
            </div>
        </Card>

    {:else if activeTab === 'preferences'}
        <Card>
            <div class="space-y-6">
                <h2 class="text-lg font-semibold text-[var(--on-surface)]">Appearance</h2>
                <div class="flex items-center justify-between">
                    <div>
                        <p class="text-sm font-medium text-[var(--on-surface)]">Theme</p>
                        <p class="text-xs text-[var(--on-surface-variant)]">Choose your preferred color scheme</p>
                    </div>
                    <div class="flex gap-2">
                        <button
                            class="px-4 py-2 rounded-lg text-sm font-medium transition-all cursor-pointer {$theme === 'light' ? 'text-[var(--on-primary)]' : 'bg-[var(--surface-container)] text-[var(--on-surface-variant)] hover:text-[var(--on-surface)]'}"
                            style={$theme === 'light' ? 'background: linear-gradient(135deg, var(--primary), var(--primary-container))' : ''}
                            onclick={() => { if ($theme !== 'light') theme.toggle(); }}
                        >
                            <span class="material-symbols-outlined align-middle mr-1" style="font-size: 16px">light_mode</span>
                            Light
                        </button>
                        <button
                            class="px-4 py-2 rounded-lg text-sm font-medium transition-all cursor-pointer {$theme === 'dark' ? 'text-[var(--on-primary)]' : 'bg-[var(--surface-container)] text-[var(--on-surface-variant)] hover:text-[var(--on-surface)]'}"
                            style={$theme === 'dark' ? 'background: linear-gradient(135deg, var(--primary), var(--primary-container))' : ''}
                            onclick={() => { if ($theme !== 'dark') theme.toggle(); }}
                        >
                            <span class="material-symbols-outlined align-middle mr-1" style="font-size: 16px">dark_mode</span>
                            Dark
                        </button>
                    </div>
                </div>
            </div>
        </Card>

    {:else if activeTab === 'security'}
        <Card>
            <div class="space-y-6">
                <h2 class="text-lg font-semibold text-[var(--on-surface)]">Change Password</h2>
                <form onsubmit={handleChangePassword} class="space-y-5 max-w-md">
                    <Input label="Current Password" type="password" bind:value={currentPassword} autocomplete="current-password" />
                    <Input label="New Password" type="password" bind:value={newPassword} autocomplete="new-password" />
                    <Input label="Confirm Password" type="password" bind:value={confirmPassword} autocomplete="new-password" />

                    {#if error}
                        <div class="flex items-center gap-2 text-sm text-[var(--error)] bg-[var(--error-container)]/20 rounded-lg px-4 py-3">
                            <span class="material-symbols-outlined" style="font-size: 18px">error</span>
                            {error}
                        </div>
                    {/if}
                    {#if success}
                        <div class="flex items-center gap-2 text-sm text-[var(--success)] bg-[var(--success-container)]/20 rounded-lg px-4 py-3">
                            <span class="material-symbols-outlined" style="font-size: 18px">check_circle</span>
                            {success}
                        </div>
                    {/if}

                    <Button type="submit" disabled={saving}>
                        {saving ? 'Saving...' : 'Update Password'}
                    </Button>
                </form>
            </div>
        </Card>

    {:else if activeTab === 'gateway' && isAdmin}
        {#if gatewayLoading}
            <Card>
                <p class="text-sm text-[var(--on-surface-variant)] py-4">Loading gateway settings...</p>
            </Card>
        {:else}
        <div class="space-y-6">
            {#if gatewayLoadError}
                <div class="flex items-center gap-2 text-sm text-[var(--error)] bg-[var(--error-container)]/20 rounded-lg px-4 py-3">
                    <span class="material-symbols-outlined" style="font-size: 18px">error</span>
                    {gatewayLoadError}
                </div>
            {/if}
            <Card>
                <div class="space-y-3">
                    <h2 class="text-lg font-semibold text-[var(--on-surface)]">Active GatewayPlugin Pipeline</h2>
                    <p class="text-xs text-[var(--on-surface-variant)]">
                        Onion-style middleware order: PreHook executes left-to-right; PostHook executes in reverse.
                    </p>
                    <div class="flex flex-wrap items-center gap-2">
                        {#if pipelinePlugins.length === 0}
                            <Badge variant="neutral" label="No plugins registered at startup (Redaction ready)" />
                        {:else}
                            {#each pipelinePlugins as plugin, i}
                                <Badge variant="info" label={`${i + 1}. ${plugin}`} />
                            {/each}
                        {/if}
                    </div>
                </div>
            </Card>

            <Card>
                <div class="space-y-5">
                    <div>
                        <h2 class="text-lg font-semibold text-[var(--on-surface)]">PII &amp; Secret Redaction</h2>
                        <p class="text-xs text-[var(--on-surface-variant)]">
                            Scrubs API keys, bearer tokens, emails, SSNs, and custom regex patterns before telemetry/log persistence.
                        </p>
                    </div>

                    <div class="grid grid-cols-2 md:grid-cols-3 gap-3 text-sm text-[var(--on-surface)]">
                        <label class="flex items-center gap-2 cursor-pointer">
                            <input type="checkbox" bind:checked={redaction.redact_api_keys} />
                            API Keys
                        </label>
                        <label class="flex items-center gap-2 cursor-pointer">
                            <input type="checkbox" bind:checked={redaction.redact_bearer_tokens} />
                            Bearer Tokens
                        </label>
                        <label class="flex items-center gap-2 cursor-pointer">
                            <input type="checkbox" bind:checked={redaction.redact_emails} />
                            Emails
                        </label>
                        <label class="flex items-center gap-2 cursor-pointer">
                            <input type="checkbox" bind:checked={redaction.redact_ssns} />
                            SSNs
                        </label>
                        <label class="flex items-center gap-2 cursor-pointer">
                            <input type="checkbox" bind:checked={redaction.redact_requests} />
                            Redact Requests
                        </label>
                        <label class="flex items-center gap-2 cursor-pointer">
                            <input type="checkbox" bind:checked={redaction.redact_responses} />
                            Redact Responses
                        </label>
                    </div>

                    <div>
                        <label for="custom-redaction" class="block text-xs font-medium uppercase tracking-wider text-[var(--on-surface-variant)] mb-1">
                            Custom Regex Rules (pattern =&gt; replacement, one per line)
                        </label>
                        <textarea
                            id="custom-redaction"
                            bind:value={customPatternsText}
                            rows="3"
                            placeholder="SECRET-[0-9]+ => [CUSTOM_SECRET]"
                            class="w-full px-3 py-2 bg-[var(--surface-container-low)] text-[var(--on-surface)] placeholder:text-[var(--outline)] rounded-lg border-b-2 border-b-[var(--outline-variant)] focus:border-b-[var(--primary)] outline-none text-xs font-mono"
                        ></textarea>
                    </div>

                    <div class="pt-4 border-t border-[var(--ghost-border)] space-y-3">
                        <div class="flex flex-wrap items-center justify-between gap-4">
                            <div>
                                <p class="text-sm font-medium text-[var(--on-surface)]">Reversible Redaction (Opt-In)</p>
                                <p class="text-xs text-[var(--on-surface-variant)]">
                                    Tokenize secrets into per-request placeholders and restore original values in the LLM response.
                                </p>
                            </div>
                            <label class="flex items-center gap-2 text-sm text-[var(--on-surface)] cursor-pointer">
                                <input type="checkbox" bind:checked={redaction.reversible} />
                                Enable Reversible Mode
                            </label>
                        </div>

                        {#if redaction.reversible}
                            <div class="max-w-xs">
                                <label for="placeholder-format" class="block text-xs font-medium uppercase tracking-wider text-[var(--on-surface-variant)] mb-1">
                                    Placeholder Format
                                </label>
                                <select
                                    id="placeholder-format"
                                    bind:value={redaction.placeholder_format}
                                    class="w-full px-3 py-2 bg-[var(--surface-container-low)] text-[var(--on-surface)] rounded-lg border-b-2 border-b-[var(--outline-variant)] focus:border-b-[var(--primary)] outline-none text-xs"
                                >
                                    <option value="indexed">Indexed ([REDACTED_API_KEY_1])</option>
                                    <option value="uuid">UUID ([REDACTED_API_KEY_a1b2c3d4])</option>
                                </select>
                            </div>
                        {/if}
                    </div>

                    <div class="pt-4 border-t border-[var(--ghost-border)] space-y-4">
                        <div class="flex flex-wrap items-center justify-between gap-4">
                            <div>
                                <p class="text-sm font-medium text-[var(--on-surface)]">Smart Redaction (Microsoft Presidio / Local LLM)</p>
                                <p class="text-xs text-[var(--on-surface-variant)]">
                                    Run a semantic PII &amp; secret detector sidecar or trusted local LLM before forwarding prompts upstream.
                                </p>
                            </div>
                            <label class="flex items-center gap-2 text-sm text-[var(--on-surface)] cursor-pointer">
                                <input type="checkbox" bind:checked={redaction.smart.enabled} />
                                Enable Smart Redaction
                            </label>
                        </div>

                        {#if redaction.smart.enabled}
                            <div class="grid grid-cols-1 md:grid-cols-3 gap-4">
                                <div>
                                    <label for="smart-backend" class="block text-xs font-medium uppercase tracking-wider text-[var(--on-surface-variant)] mb-1">
                                        Backend
                                    </label>
                                    <select
                                        id="smart-backend"
                                        bind:value={redaction.smart.backend}
                                        class="w-full px-3 py-2 bg-[var(--surface-container-low)] text-[var(--on-surface)] rounded-lg border-b-2 border-b-[var(--outline-variant)] focus:border-b-[var(--primary)] outline-none text-xs"
                                    >
                                        <option value="presidio">Microsoft Presidio Analyzer</option>
                                        <option value="llm">Local / Trusted LLM</option>
                                    </select>
                                </div>
                                <div>
                                    <label for="smart-fail-mode" class="block text-xs font-medium uppercase tracking-wider text-[var(--on-surface-variant)] mb-1">
                                        Failure Mode
                                    </label>
                                    <select
                                        id="smart-fail-mode"
                                        bind:value={redaction.smart.fail_mode}
                                        class="w-full px-3 py-2 bg-[var(--surface-container-low)] text-[var(--on-surface)] rounded-lg border-b-2 border-b-[var(--outline-variant)] focus:border-b-[var(--primary)] outline-none text-xs"
                                    >
                                        <option value="fallback_regex">Fallback to Regex</option>
                                        <option value="fail_closed">Fail Closed (HTTP 503)</option>
                                    </select>
                                </div>
                                <Input label="Timeout (ms)" type="number" bind:value={smartTimeoutText} />
                            </div>

                            {#if redaction.smart.backend === 'presidio'}
                                <div class="grid grid-cols-1 md:grid-cols-2 gap-4">
                                    <Input label="Presidio Analyzer URL" bind:value={redaction.smart.presidio_url} placeholder="http://127.0.0.1:5001" />
                                    <Input label="Language" bind:value={redaction.smart.presidio_language} placeholder="en" />
                                    <Input label="Score Threshold (0.0 - 1.0)" type="text" bind:value={presidioScoreText} />
                                    <Input label="Entities (comma-separated)" bind:value={presidioEntitiesText} placeholder="PERSON, LOCATION, PHONE_NUMBER" />
                                </div>
                            {:else}
                                <div class="grid grid-cols-1 md:grid-cols-2 gap-4">
                                    <Input label="LLM Base URL (OpenAI-compatible)" bind:value={redaction.smart.llm_base_url} placeholder="http://127.0.0.1:11434/v1" />
                                    <Input label="LLM Model" bind:value={redaction.smart.llm_model} placeholder="qwen2.5:3b" />
                                    <Input label="API Key (optional)" type="password" bind:value={redaction.smart.llm_api_key} placeholder="Leave empty for local Ollama/vLLM" />
                                    <Input label="Categories (comma-separated)" bind:value={llmCategoriesText} placeholder="pii, secrets, financial, internal_names" />
                                </div>
                            {/if}
                        {/if}
                    </div>

                    {#if redactionError}
                        <p class="text-xs text-[var(--error)]">{redactionError}</p>
                    {/if}
                    {#if redactionStatus}
                        <p class="text-xs text-[var(--success)]">{redactionStatus}</p>
                    {/if}

                    <div class="flex justify-end">
                        <Button size="sm" disabled={savingRedaction} onclick={saveRedaction}>
                            {savingRedaction ? 'Saving...' : 'Save Redaction'}
                        </Button>
                    </div>

                    <div class="pt-4 border-t border-[var(--ghost-border)] space-y-3">
                        <p class="text-xs font-medium uppercase tracking-wider text-[var(--on-surface-variant)]">
                            Live Redaction Preview
                        </p>
                        <div class="flex gap-2">
                            <input
                                type="text"
                                bind:value={testSample}
                                placeholder="Paste sample text (e.g. alice@example.com SECRET-1234)"
                                class="flex-1 px-3 py-2 bg-[var(--surface-container-low)] text-[var(--on-surface)] placeholder:text-[var(--outline)] rounded-lg border-b-2 border-b-[var(--outline-variant)] focus:border-b-[var(--primary)] outline-none text-xs font-mono"
                            />
                            <Button variant="secondary" size="sm" onclick={runRedactionTest}>
                                Test Redaction
                            </Button>
                        </div>
                        {#if testError}
                            <p class="text-xs text-[var(--error)]">{testError}</p>
                        {/if}
                        {#if testOutput}
                            <div class="p-3 rounded-lg bg-[var(--surface-container-low)] text-xs font-mono text-[var(--on-surface)]">
                                {testOutput}
                            </div>
                        {/if}
                        {#if testVaultEntries}
                            <div class="p-3 rounded-lg bg-[var(--surface-container-low)] space-y-1">
                                <p class="text-[10px] font-medium uppercase tracking-wider text-[var(--on-surface-variant)]">
                                    Reversible Vault Mappings
                                </p>
                                {#each Object.entries(testVaultEntries) as [placeholder, original]}
                                    <div class="text-xs font-mono text-[var(--on-surface)]">
                                        <span class="text-[var(--primary)]">{placeholder}</span> → {original}
                                    </div>
                                {/each}
                            </div>
                        {/if}
                    </div>
                </div>
            </Card>

            <Card>
                <div class="space-y-5">
                    <div>
                        <h2 class="text-lg font-semibold text-[var(--on-surface)]">Semantic Similarity Cache</h2>
                        <p class="text-xs text-[var(--on-surface-variant)]">
                            Configure two-tier exact + semantic similarity caching and SSE streaming cache replay.
                        </p>
                    </div>

                    <div class="flex flex-wrap gap-6 text-sm text-[var(--on-surface)]">
                        <label class="flex items-center gap-2 cursor-pointer">
                            <input type="checkbox" bind:checked={cache.enabled} />
                            Enable Semantic Cache
                        </label>
                        <label class="flex items-center gap-2 cursor-pointer">
                            <input type="checkbox" bind:checked={cache.sse_replay} />
                            Enable SSE Streaming Replay
                        </label>
                    </div>

                    <div class="grid grid-cols-1 md:grid-cols-2 gap-4">
                        <Input label="Cosine Similarity Threshold (0.50 - 1.00)" type="text" bind:value={cacheThresholdText} />
                        <Input label="Cache TTL (seconds)" type="number" bind:value={cacheTtlText} />
                        <Input label="Embedding Provider" bind:value={cache.embedding_provider} />
                        <Input label="Embedding Model" bind:value={cache.embedding_model} />
                    </div>

                    {#if cacheError}
                        <p class="text-xs text-[var(--error)]">{cacheError}</p>
                    {/if}
                    {#if cacheStatus}
                        <p class="text-xs text-[var(--success)]">{cacheStatus}</p>
                    {/if}

                    <div class="flex justify-end">
                        <Button size="sm" disabled={savingCache} onclick={saveCache}>
                            {savingCache ? 'Saving...' : 'Save Cache Settings'}
                        </Button>
                    </div>
                </div>
            </Card>
        </div>
        {/if}
    {/if}
</div>