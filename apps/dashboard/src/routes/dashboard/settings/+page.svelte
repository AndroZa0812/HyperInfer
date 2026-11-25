<script lang="ts">
    import { auth } from '#lib/stores/auth';
    import { theme } from '#lib/stores/theme';
    import { api } from '#lib/api';
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

    const tabs = [
        { label: 'Profile', value: 'profile' },
        { label: 'Preferences', value: 'preferences' },
        { label: 'Security', value: 'security' },
    ];

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
    {/if}
</div>