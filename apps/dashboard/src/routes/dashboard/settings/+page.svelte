<script lang="ts">
    import { auth } from '$lib/stores/auth';
    import { theme } from '$lib/stores/theme';
    import { api } from '$lib/api';
    import Card from '$lib/components/Card.svelte';
    import Button from '$lib/components/Button.svelte';
    import Input from '$lib/components/Input.svelte';
    import Badge from '$lib/components/Badge.svelte';
    import TabBar from '$lib/components/TabBar.svelte';

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
        <div class="grid grid-cols-1 lg:grid-cols-[1fr_280px] gap-6">
            <Card>
                <div class="space-y-6">
                    <h2 class="text-lg font-semibold text-[var(--on-surface)]">Personal Information</h2>
                    <p class="text-sm text-[var(--on-surface-variant)]">Update your account identity and contact details.</p>

                    <div class="grid grid-cols-2 gap-4">
                        <Input label="First Name" value="" placeholder="First name" disabled />
                        <Input label="Last Name" value="" placeholder="Last name" disabled />
                    </div>

                    <Input label="Email Address" value={$auth.user?.email || ''} disabled />

                    <div>
                        <label for="bio" class="block text-xs font-medium uppercase tracking-wider text-[var(--on-surface-variant)] mb-1.5">Professional Bio</label>
                        <textarea
                            id="bio"
                            class="w-full px-4 py-3 bg-[var(--surface-container-low)] text-[var(--on-surface)] placeholder:text-[var(--outline)] rounded-lg border-b-2 border-b-[var(--outline-variant)] focus:border-b-[var(--primary)] outline-none transition-colors resize-none h-24 text-sm"
                            placeholder="Tell us about yourself..."
                        ></textarea>
                    </div>

                    <div class="flex items-center gap-4">
                        <div class="w-16 h-16 rounded-full flex items-center justify-center text-xl font-bold text-[var(--on-primary)]" style="background: linear-gradient(135deg, var(--primary), var(--primary-container))">
                            {$auth.user?.email?.[0]?.toUpperCase() || '?'}
                        </div>
                        <div>
                            <p class="text-sm font-medium text-[var(--on-surface)]">Profile Picture</p>
                            <p class="text-xs text-[var(--on-surface-variant)]">PNG, JPG or GIF. Max 2MB.</p>
                            <div class="flex gap-2 mt-2">
                                <Button variant="secondary" size="sm">Upload New</Button>
                                <Button variant="ghost" size="sm">Remove</Button>
                            </div>
                        </div>
                    </div>

                    <div class="flex gap-3 justify-end pt-4 border-t border-[var(--ghost-border)]">
                        <Button variant="ghost">Cancel</Button>
                        <Button>Save Changes</Button>
                    </div>
                </div>
            </Card>

            <div class="space-y-4">
                <Card>
                    <div class="flex items-center gap-3 mb-4">
                        <span class="material-symbols-outlined text-[var(--success)]" style="font-size: 20px">verified_user</span>
                        <div>
                            <p class="text-sm font-medium text-[var(--on-surface)]">Account Verified</p>
                            <p class="text-xs text-[var(--on-surface-variant)]">Enterprise identity verified</p>
                        </div>
                    </div>
                </Card>

                <Card>
                    <h3 class="text-xs font-semibold uppercase tracking-wider text-[var(--on-surface-variant)] mb-4">Quick Stats</h3>
                    <div class="space-y-3">
                        <div class="flex items-center justify-between">
                            <span class="text-sm text-[var(--on-surface-variant)]">Keys Managed</span>
                            <span class="text-sm font-semibold text-[var(--on-surface)]">12</span>
                        </div>
                        <div class="flex items-center justify-between">
                            <span class="text-sm text-[var(--on-surface-variant)]">Active Sessions</span>
                            <span class="text-sm font-semibold text-[var(--on-surface)]">3</span>
                        </div>
                        <div class="flex items-center justify-between">
                            <span class="text-sm text-[var(--on-surface-variant)]">Role</span>
                            <Badge variant="info" label={$auth.user?.role || 'member'} />
                        </div>
                    </div>
                </Card>

                <Card>
                    <div class="flex items-center gap-2">
                        <span class="material-symbols-outlined text-[var(--primary)]" style="font-size: 18px">cloud</span>
                        <div>
                            <p class="text-sm font-medium text-[var(--on-surface)]">HyperInfer Cloud</p>
                            <p class="text-xs text-[var(--on-surface-variant)]">Enterprise Tier Active</p>
                        </div>
                    </div>
                </Card>
            </div>
        </div>

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
