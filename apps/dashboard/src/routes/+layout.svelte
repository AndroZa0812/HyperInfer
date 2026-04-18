<script lang="ts">
    import '../app.css';
    import { onMount } from 'svelte';
    import { theme } from '$lib/stores/theme';
    import { auth } from '$lib/stores/auth';

    onMount(() => {
        theme.init();
        auth.init();
    });
</script>

<slot />
