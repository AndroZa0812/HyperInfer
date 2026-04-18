<script lang="ts">
    import { goto } from '$app/navigation';
    import { auth } from '$lib/stores/auth';
    import Sidebar from '$lib/components/Sidebar.svelte';

    $: if (!$auth.loading && !$auth.user) {
        goto('/login');
    }
</script>

{#if $auth.user}
    <Sidebar>
        <slot />
    </Sidebar>
{/if}