<script lang="ts">
    import '../app.css';
    import { onMount } from 'svelte';
    import { theme } from '$lib/stores/theme';

    onMount(() => {
        theme.init();
    });
</script>

<slot />
