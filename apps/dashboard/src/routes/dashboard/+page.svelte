<script lang="ts">
    import { goto } from '$app/navigation';
    import { auth } from '$lib/stores/auth';

    let redirected = $state(false);

    $effect(() => {
        if (!$auth.loading && $auth.user && !redirected) {
            redirected = true;
            if ($auth.user.role === 'admin') {
                goto('/dashboard/teams');
            } else {
                goto('/dashboard/keys');
            }
        }
    });
</script>

<p>Redirecting...</p>