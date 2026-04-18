<script lang="ts">
    import { goto } from '$app/navigation';
    import { auth } from '$lib/stores/auth';
    import Sidebar from '$lib/components/Sidebar.svelte';
    import type { Snippet } from 'svelte';

    interface Props {
        children: Snippet;
    }

    let { children } = $props<Props>();

    $effect(() => {
        if (!$auth.loading && !$auth.user) {
            goto('/login');
        }
    });
</script>

{#if $auth.user}
    <Sidebar>
        {@render children()}
    </Sidebar>
{/if}