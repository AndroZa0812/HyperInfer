<script lang="ts">
    import { api } from '#lib/api';
    import { auth } from '#lib/stores/auth';
    import type { ApiKey, ServerConfig, Team, UsageData, User } from '#lib/types';
    import StatCard from '#lib/components/StatCard.svelte';
    import Card from '#lib/components/Card.svelte';
    import Button from '#lib/components/Button.svelte';
    import Badge from '#lib/components/Badge.svelte';

    let loading = $state(true);
    let usage = $state<UsageData | null>(null);
    let keys = $state<ApiKey[]>([]);
    let teams = $state<Team[]>([]);
    let users = $state<User[]>([]);
    let serverConfig = $state<ServerConfig | null>(null);

    let isAdmin = $derived($auth.user?.role === 'admin');
    let activeKeys = $derived(keys.filter((k) => k.is_active).length);
    let totalTokens = $derived(
        usage ? usage.total_input_tokens + usage.total_output_tokens : 0
    );

    $effect(() => {
        if ($auth.loading || !$auth.user) return;
        let cancelled = false;
        loading = true;

        const teamScope = isAdmin ? undefined : $auth.user.team_id;
        Promise.all([
            api.getUsage(teamScope).catch(() => null),
            api.getKeys(teamScope).catch(() => []),
            isAdmin ? api.getTeams().catch(() => []) : Promise.resolve([]),
            isAdmin ? api.getUsers().catch(() => []) : Promise.resolve([]),
            api.getServerConfig().catch(() => null),
        ]).then(([u, k, t, us, cfg]) => {
            if (cancelled) return;
            usage = u;
            keys = k;
            teams = t;
            users = us;
            serverConfig = cfg;
            loading = false;
        });

        return () => {
            cancelled = true;
        };
    });
</script>

<div class="space-y-8">
    <div class="flex items-center justify-between">
        <div>
            <p class="text-xs uppercase tracking-[0.2em] text-[var(--on-surface-variant)] mb-1">
                Control Plane
            </p>
            <h1 class="text-3xl font-bold text-[var(--on-surface)]">Overview</h1>
            <p class="text-sm text-[var(--on-surface-variant)] mt-1">
                Live gateway usage, credentials, and organization summary.
            </p>
        </div>
        <div class="flex gap-2">
            <Button href="/dashboard/keys" size="sm">
                <span class="material-symbols-outlined" style="font-size: 18px">vpn_key</span>
                Manage Keys
            </Button>
            {#if isAdmin}
                <Button href="/dashboard/routing" variant="secondary" size="sm">
                    <span class="material-symbols-outlined" style="font-size: 18px">route</span>
                    Routing
                </Button>
            {/if}
        </div>
    </div>

    {#if loading}
        <div class="flex items-center justify-center h-64">
            <span class="material-symbols-outlined animate-spin text-[var(--primary)]" style="font-size: 32px">
                progress_activity
            </span>
        </div>
    {:else}
        <div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-4">
            <StatCard
                icon="speed"
                label="Total Requests"
                value={usage ? usage.total_requests.toLocaleString() : '0'}
                trend="Gateway traffic"
                trendDirection="neutral"
            />
            <StatCard
                icon="data_usage"
                label="Total Tokens"
                value={totalTokens.toLocaleString()}
                trend="Input + output"
                trendDirection="neutral"
            />
            <StatCard
                icon="timer"
                label="Avg Latency"
                value={usage ? `${Math.round(usage.avg_response_time_ms)}ms` : '0ms'}
                trend="Response time"
                trendDirection="neutral"
            />
            <StatCard
                icon="vpn_key"
                label="Active Keys"
                value={`${activeKeys} / ${keys.length}`}
                trend="Virtual API keys"
                trendDirection="up"
            />
        </div>

        {#if isAdmin}
            <div class="grid grid-cols-1 md:grid-cols-2 gap-4">
                <Card>
                    <div class="flex items-center justify-between">
                        <div>
                            <p class="text-xs uppercase tracking-wider text-[var(--on-surface-variant)]">
                                Organization
                            </p>
                            <h2 class="text-2xl font-bold text-[var(--on-surface)] mt-1">
                                {teams.length} {teams.length === 1 ? 'Team' : 'Teams'}
                            </h2>
                            <p class="text-sm text-[var(--on-surface-variant)] mt-1">
                                Manage budgets, rate-limit quotas, and model aliases.
                            </p>
                        </div>
                        <Button href="/dashboard/teams" variant="secondary" size="sm">
                            View Teams
                        </Button>
                    </div>
                </Card>

                <Card>
                    <div class="flex items-center justify-between">
                        <div>
                            <p class="text-xs uppercase tracking-wider text-[var(--on-surface-variant)]">
                                Access Control
                            </p>
                            <h2 class="text-2xl font-bold text-[var(--on-surface)] mt-1">
                                {users.length} {users.length === 1 ? 'User' : 'Users'}
                            </h2>
                            <p class="text-sm text-[var(--on-surface-variant)] mt-1">
                                Provision accounts and manage admin/member roles.
                            </p>
                        </div>
                        <Button href="/dashboard/users" variant="secondary" size="sm">
                            Manage Users
                        </Button>
                    </div>
                </Card>
            </div>
        {/if}

        {#if serverConfig}
            <Card>
                <div class="flex flex-wrap items-center justify-between gap-4">
                    <div class="space-y-1.5">
                        <div class="flex items-center gap-2">
                            <p class="text-xs uppercase tracking-wider text-[var(--on-surface-variant)]">
                                Gateway Runtime
                            </p>
                            <Badge
                                variant={serverConfig.standalone ? 'warning' : 'success'}
                                label={serverConfig.standalone ? 'Standalone Mode' : 'Standard Mode'}
                            />
                        </div>
                        <div class="flex flex-wrap items-center gap-2 text-xs text-[var(--on-surface-variant)]">
                            <span>Auto-detected providers:</span>
                            {#if serverConfig.detected_providers.length > 0}
                                {#each serverConfig.detected_providers as provider}
                                    <Badge variant="info" label={provider.name} />
                                {/each}
                            {:else}
                                <span class="text-[var(--on-surface)]">None from env (configure deployments in Routing)</span>
                            {/if}
                        </div>
                    </div>
                    <div class="flex gap-2">
                        <Button href="/dashboard/playground" variant="secondary" size="sm">
                            <span class="material-symbols-outlined" style="font-size: 18px">terminal</span>
                            Open Playground
                        </Button>
                        {#if isAdmin}
                            <Button href="/dashboard/mcp" variant="secondary" size="sm">
                                <span class="material-symbols-outlined" style="font-size: 18px">hub</span>
                                MCP Gateway
                            </Button>
                        {/if}
                    </div>
                </div>
            </Card>
        {/if}

        <Card>
            <div class="space-y-4">
                <h2 class="text-lg font-semibold text-[var(--on-surface)]">Quick Actions</h2>
                <div class="flex flex-wrap gap-3">
                    <Button href="/dashboard/keys" variant="secondary" size="sm">
                        <span class="material-symbols-outlined" style="font-size: 18px">vpn_key</span>
                        API Keys
                    </Button>
                    {#if isAdmin}
                        <Button href="/dashboard/teams" variant="secondary" size="sm">
                            <span class="material-symbols-outlined" style="font-size: 18px">group</span>
                            Teams
                        </Button>
                        <Button href="/dashboard/users" variant="secondary" size="sm">
                            <span class="material-symbols-outlined" style="font-size: 18px">manage_accounts</span>
                            Users
                        </Button>
                        <Button href="/dashboard/routing" variant="secondary" size="sm">
                            <span class="material-symbols-outlined" style="font-size: 18px">route</span>
                            Routing Deployments
                        </Button>
                    {/if}
                    <Button href="/dashboard/settings" variant="secondary" size="sm">
                        <span class="material-symbols-outlined" style="font-size: 18px">settings</span>
                        Settings
                    </Button>
                </div>
            </div>
        </Card>
    {/if}
</div>
