<script lang="ts">
    import { auth } from '$lib/stores/auth';
    import { theme } from '$lib/stores/theme';
    import { api } from '$lib/api';

    let currentPassword = '';
    let newPassword = '';
    let confirmPassword = '';
    let saving = false;
    let success = '';
    let error = '';

    async function handleChangePassword() {
        error = '';
        success = '';

        if (!currentPassword) {
            error = 'Current password is required';
            return;
        }

        if (newPassword.length < 8) {
            error = 'New password must be at least 8 characters';
            return;
        }

        if (newPassword !== confirmPassword) {
            error = 'Passwords do not match';
            return;
        }

        saving = true;
        try {
            await api.changePassword(currentPassword, newPassword);
            success = 'Password updated successfully';
            currentPassword = '';
            newPassword = '';
            confirmPassword = '';
        } catch (e: any) {
            if (e.message?.includes('401')) {
                error = 'Current password is incorrect';
            } else {
                error = 'Failed to update password';
            }
        } finally {
            saving = false;
        }
    }
</script>

<div class="space-y-6 max-w-2xl">
    <h1 class="text-2xl font-bold">Settings</h1>

    <div class="bg-[var(--bg-primary)] rounded-xl p-6 space-y-6">
        <h2 class="text-lg font-medium">Profile</h2>
        <div>
            <label class="block text-sm font-medium mb-1">Email</label>
            <input
                type="email"
                value={$auth.user?.email || ''}
                disabled
                class="w-full px-4 py-2 border rounded-lg opacity-60"
            />
        </div>
        <div>
            <label class="block text-sm font-medium mb-1">Role</label>
            <input
                type="text"
                value={$auth.user?.role || ''}
                disabled
                class="w-full px-4 py-2 border rounded-lg opacity-60"
            />
        </div>
    </div>

    <div class="bg-[var(--bg-primary)] rounded-xl p-6 space-y-6">
        <h2 class="text-lg font-medium">Appearance</h2>
        <div class="flex items-center gap-4">
            <span>Theme:</span>
            <button
                on:click={() => theme.toggle()}
                class="px-4 py-2 border rounded-lg"
            >
                {$theme === 'light' ? 'Switch to Dark' : 'Switch to Light'}
            </button>
        </div>
    </div>

    <div class="bg-[var(--bg-primary)] rounded-xl p-6 space-y-6">
        <h2 class="text-lg font-medium">Change Password</h2>
        <form on:submit|preventDefault={handleChangePassword} class="space-y-4">
            <div>
                <label class="block text-sm font-medium mb-1">Current Password</label>
                <input
                    type="password"
                    bind:value={currentPassword}
                    class="w-full px-4 py-2 border rounded-lg"
                />
            </div>
            <div>
                <label class="block text-sm font-medium mb-1">New Password</label>
                <input
                    type="password"
                    bind:value={newPassword}
                    class="w-full px-4 py-2 border rounded-lg"
                />
            </div>
            <div>
                <label class="block text-sm font-medium mb-1">Confirm Password</label>
                <input
                    type="password"
                    bind:value={confirmPassword}
                    class="w-full px-4 py-2 border rounded-lg"
                />
            </div>
            {#if error}
                <p class="text-red-500 text-sm">{error}</p>
            {/if}
            {#if success}
                <p class="text-green-600 text-sm">{success}</p>
            {/if}
            <button
                type="submit"
                class="px-4 py-2 bg-[var(--accent)] text-white rounded-lg"
                disabled={saving}
            >
                {saving ? 'Saving...' : 'Update Password'}
            </button>
        </form>
    </div>
</div>