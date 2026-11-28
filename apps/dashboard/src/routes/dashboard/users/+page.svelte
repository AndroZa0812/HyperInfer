<script lang="ts">
    import { api } from '#lib/api';
    import { auth } from '#lib/stores/auth';
    import type { Team, User } from '#lib/types';
    import Button from '@hyperinfer/ui/Button.svelte';
    import Input from '@hyperinfer/ui/Input.svelte';
    import Modal from '@hyperinfer/ui/Modal.svelte';
    import Badge from '@hyperinfer/ui/Badge.svelte';
    import Card from '@hyperinfer/ui/Card.svelte';

    let users = $state<User[]>([]);
    let teams = $state<Team[]>([]);
    let loading = $state(true);
    let loadError = $state<string | null>(null);
    let searchQuery = $state('');
    let teamFilter = $state('all');

    let showCreate = $state(false);
    let newEmail = $state('');
    let newPassword = $state('');
    let newRole = $state<'admin' | 'member'>('member');
    let newTeamId = $state('');
    let createError = $state('');
    let creating = $state(false);

    let pendingDeleteUser = $state<User | null>(null);
    let showDeleteConfirm = $state(false);
    let deleteError = $state('');
    let roleError = $state('');

    let teamNames = $derived(new Map(teams.map((t) => [t.id, t.name])));

    let filteredUsers = $derived(
        users.filter((u) => {
            const matchesSearch =
                !searchQuery || u.email.toLowerCase().includes(searchQuery.toLowerCase());
            const matchesTeam = teamFilter === 'all' || u.team_id === teamFilter;
            return matchesSearch && matchesTeam;
        })
    );

    $effect(() => {
        if ($auth.loading || !$auth.user) return;
        if ($auth.user.role !== 'admin') {
            loading = false;
            return;
        }
        let cancelled = false;
        loading = true;
        loadError = null;
        Promise.all([api.getUsers(), api.getTeams()])
            .then(([u, t]) => {
                if (cancelled) return;
                users = u;
                teams = t;
                if (!newTeamId && t.length > 0) {
                    newTeamId = t[0].id;
                }
                loading = false;
            })
            .catch((e) => {
                if (cancelled) return;
                console.error('Failed to load users', e);
                loadError = 'Failed to load users. Please try again.';
                loading = false;
            });
        return () => {
            cancelled = true;
        };
    });

    function openCreateModal() {
        createError = '';
        newEmail = '';
        newPassword = '';
        newRole = 'member';
        if (!newTeamId && teams.length > 0) {
            newTeamId = teams[0].id;
        }
        showCreate = true;
    }

    async function createUser(e: Event) {
        e.preventDefault();
        createError = '';
        const email = newEmail.trim();
        if (!email || !email.includes('@')) {
            createError = 'Valid email is required';
            return;
        }
        if (!newTeamId) {
            createError = 'Please select a team';
            return;
        }
        if (newPassword && newPassword.length < 8) {
            createError = 'Password must be at least 8 characters';
            return;
        }

        creating = true;
        try {
            const created = await api.createUser({
                email,
                team_id: newTeamId,
                role: newRole,
                password: newPassword || undefined,
            });
            users = [...users, created];
            showCreate = false;
        } catch (err) {
            console.error('Failed to create user', err);
            createError = 'Failed to create user. Email may already exist.';
        } finally {
            creating = false;
        }
    }

    async function toggleRole(user: User) {
        roleError = '';
        const nextRole: 'admin' | 'member' = user.role === 'admin' ? 'member' : 'admin';
        try {
            const updated = await api.updateUserRole(user.id, nextRole);
            users = users.map((u) => (u.id === user.id ? updated : u));
        } catch (err) {
            console.error('Failed to update role', err);
            roleError = 'Failed to update user role.';
        }
    }

    function confirmDelete(user: User) {
        pendingDeleteUser = user;
        deleteError = '';
        showDeleteConfirm = true;
    }

    function cancelDelete() {
        pendingDeleteUser = null;
        deleteError = '';
        showDeleteConfirm = false;
    }

    async function deleteUser() {
        if (!pendingDeleteUser) return;
        deleteError = '';
        try {
            await api.deleteUser(pendingDeleteUser.id);
            users = users.filter((u) => u.id !== pendingDeleteUser!.id);
            pendingDeleteUser = null;
            showDeleteConfirm = false;
        } catch (err) {
            console.error('Failed to delete user', err);
            deleteError = 'Failed to delete user. Please try again.';
        }
    }
</script>

{#if $auth.user && $auth.user.role !== 'admin'}
    <Card>
        <p class="text-sm text-[var(--error)]">Admin access is required to manage users.</p>
    </Card>
{:else}
    <div class="space-y-8">
        <div class="flex items-center justify-between">
            <div>
                <h1 class="text-2xl font-bold text-[var(--on-surface)]">Users</h1>
                <p class="text-sm text-[var(--on-surface-variant)] mt-1">
                    Provision accounts, assign teams, and manage admin/member roles.
                </p>
            </div>
            <Button onclick={openCreateModal}>
                <span class="material-symbols-outlined" style="font-size: 18px">person_add</span>
                Create User
            </Button>
        </div>

        {#if roleError}
            <div class="text-sm text-[var(--error)] bg-[var(--error-container)]/20 rounded-lg px-4 py-3">
                {roleError}
            </div>
        {/if}

        <Card padding="none">
            <div class="p-4 flex flex-wrap items-center gap-4">
                <div class="flex-1 min-w-[200px] relative">
                    <span class="material-symbols-outlined absolute left-3 top-1/2 -translate-y-1/2 text-[var(--outline)]" style="font-size: 20px">
                        search
                    </span>
                    <input
                        bind:value={searchQuery}
                        placeholder="Search users by email..."
                        class="w-full pl-10 pr-4 py-2.5 bg-[var(--surface-container-low)] text-[var(--on-surface)] placeholder:text-[var(--outline)] rounded-lg border-b-2 border-b-[var(--outline-variant)] focus:border-b-[var(--primary)] outline-none transition-colors text-sm"
                    />
                </div>
                <select
                    aria-label="Filter by team"
                    bind:value={teamFilter}
                    class="px-3 py-2.5 bg-[var(--surface-container-low)] text-[var(--on-surface)] rounded-lg border-b-2 border-b-[var(--outline-variant)] focus:border-b-[var(--primary)] outline-none text-sm"
                >
                    <option value="all">All Teams</option>
                    {#each teams as team}
                        <option value={team.id}>{team.name}</option>
                    {/each}
                </select>
            </div>

            {#if loading}
                <div class="flex items-center justify-center h-32">
                    <span class="material-symbols-outlined animate-spin text-[var(--primary)]" style="font-size: 24px">
                        progress_activity
                    </span>
                </div>
            {:else if loadError}
                <div class="p-8 text-center text-[var(--error)] text-sm">{loadError}</div>
            {:else if filteredUsers.length === 0}
                <div class="p-8 text-center text-[var(--on-surface-variant)]">No users found</div>
            {:else}
                <div class="overflow-x-auto">
                    <table class="w-full">
                        <thead>
                            <tr class="bg-[var(--surface-container-low)]">
                                <th class="px-5 py-3.5 text-left text-xs font-medium uppercase tracking-wider text-[var(--on-surface-variant)]">Email</th>
                                <th class="px-5 py-3.5 text-left text-xs font-medium uppercase tracking-wider text-[var(--on-surface-variant)]">Team</th>
                                <th class="px-5 py-3.5 text-left text-xs font-medium uppercase tracking-wider text-[var(--on-surface-variant)]">Role</th>
                                <th class="px-5 py-3.5 text-right text-xs font-medium uppercase tracking-wider text-[var(--on-surface-variant)]">Actions</th>
                            </tr>
                        </thead>
                        <tbody>
                            {#each filteredUsers as user, i}
                                {@const isSelf = user.id === $auth.user?.id}
                                <tr class="{i > 0 ? 'border-t border-[var(--ghost-border)]' : ''} hover:bg-[var(--surface-container-low)]/50 transition-colors">
                                    <td class="px-5 py-4 text-sm font-medium text-[var(--on-surface)]">
                                        {user.email}
                                        {#if isSelf}
                                            <span class="ml-2 text-xs text-[var(--on-surface-variant)]">(You)</span>
                                        {/if}
                                    </td>
                                    <td class="px-5 py-4 text-sm text-[var(--on-surface-variant)]">
                                        {teamNames.get(user.team_id) ?? user.team_id.slice(0, 8)}
                                    </td>
                                    <td class="px-5 py-4">
                                        <Badge variant={user.role === 'admin' ? 'info' : 'neutral'} label={user.role} />
                                    </td>
                                    <td class="px-5 py-4 text-right">
                                        {#if !isSelf}
                                            <div class="flex items-center justify-end gap-2">
                                                <Button variant="secondary" size="sm" onclick={() => toggleRole(user)}>
                                                    {user.role === 'admin' ? 'Make Member' : 'Make Admin'}
                                                </Button>
                                                <Button variant="ghost" size="sm" class="text-[var(--error)]" onclick={() => confirmDelete(user)}>
                                                    Delete
                                                </Button>
                                            </div>
                                        {:else}
                                            <span class="text-xs text-[var(--outline)]">Current session</span>
                                        {/if}
                                    </td>
                                </tr>
                            {/each}
                        </tbody>
                    </table>
                </div>
            {/if}
        </Card>
    </div>

    <Modal bind:open={showCreate}>
        <form class="p-6 space-y-5" onsubmit={createUser}>
            <div>
                <h2 class="text-xl font-semibold text-[var(--on-surface)]">Create User</h2>
                <p class="text-sm text-[var(--on-surface-variant)] mt-1">
                    Add a new user account to a team.
                </p>
            </div>
            <Input id="user-email" label="Email Address" type="email" bind:value={newEmail} placeholder="member@example.com" required />
            <div>
                <label for="user-team" class="block text-xs font-medium uppercase tracking-wider text-[var(--on-surface-variant)] mb-1.5">
                    Team
                </label>
                <select
                    id="user-team"
                    bind:value={newTeamId}
                    class="w-full px-4 py-3 bg-[var(--surface-container-low)] text-[var(--on-surface)] rounded-lg border-b-2 border-b-[var(--outline-variant)] focus:border-b-[var(--primary)] outline-none"
                >
                    {#each teams as team}
                        <option value={team.id}>{team.name}</option>
                    {/each}
                </select>
            </div>
            <div>
                <label for="user-role" class="block text-xs font-medium uppercase tracking-wider text-[var(--on-surface-variant)] mb-1.5">
                    Role
                </label>
                <select
                    id="user-role"
                    bind:value={newRole}
                    class="w-full px-4 py-3 bg-[var(--surface-container-low)] text-[var(--on-surface)] rounded-lg border-b-2 border-b-[var(--outline-variant)] focus:border-b-[var(--primary)] outline-none"
                >
                    <option value="member">Member</option>
                    <option value="admin">Admin</option>
                </select>
            </div>
            <Input
                id="user-password"
                label="Initial Password"
                type="password"
                bind:value={newPassword}
                placeholder="Min 8 characters"
                required
            />
            {#if createError}
                <div class="text-sm text-[var(--error)]">{createError}</div>
            {/if}
            <div class="flex gap-3 justify-end">
                <Button variant="ghost" onclick={() => (showCreate = false)}>Cancel</Button>
                <Button type="submit" disabled={creating}>
                    {creating ? 'Creating...' : 'Create User'}
                </Button>
            </div>
        </form>
    </Modal>

    <Modal bind:open={showDeleteConfirm} onclose={cancelDelete}>
        <div class="p-6">
            <h2 class="text-xl font-bold text-[var(--on-surface)] mb-2">Delete User</h2>
            <p class="text-sm text-[var(--on-surface-variant)] mb-4">
                Are you sure you want to delete <strong>{pendingDeleteUser?.email}</strong>? This action cannot be undone.
            </p>
            {#if deleteError}
                <div class="text-sm text-[var(--error)] mb-4">{deleteError}</div>
            {/if}
            <div class="flex justify-end gap-2">
                <Button variant="ghost" onclick={cancelDelete}>Cancel</Button>
                <button
                    type="button"
                    class="inline-flex items-center justify-center font-medium rounded-lg px-5 py-2.5 text-sm bg-[var(--error)] text-[var(--on-error)] hover:opacity-90 transition-all cursor-pointer"
                    onclick={deleteUser}
                >
                    Delete
                </button>
            </div>
        </div>
    </Modal>
{/if}
