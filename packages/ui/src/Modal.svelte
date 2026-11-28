<script lang="ts">
	import type { Snippet } from 'svelte';

	interface Props {
		open?: boolean;
		size?: 'md' | 'lg' | 'xl';
		onclose?: () => void;
		children: Snippet;
	}

	let { open = $bindable(false), size = 'md', onclose, children }: Props = $props();

	const sizeClass = $derived(
		size === 'xl' ? 'max-w-3xl' : size === 'lg' ? 'max-w-2xl' : 'max-w-lg'
	);

	function handleBackdrop(e: MouseEvent) {
		if (e.target === e.currentTarget) {
			open = false;
			onclose?.();
		}
	}

	function handleKeydown(e: KeyboardEvent) {
		if (e.key === 'Escape') {
			open = false;
			onclose?.();
		}
	}
</script>

<svelte:window onkeydown={handleKeydown} />

{#if open}
	<!-- svelte-ignore a11y_click_events_have_key_events a11y_no_noninteractive_element_interactions -->
	<div
		class="fixed inset-0 z-50 flex items-center justify-center p-4"
		onclick={handleBackdrop}
		role="dialog"
		aria-modal="true"
		tabindex="-1"
	>
		<div class="absolute inset-0 bg-[var(--on-surface)]/20 backdrop-blur-sm"></div>
		<div class="relative bg-[var(--surface-container-lowest)] border border-[var(--ghost-border)] rounded-xl shadow-[0_24px_48px_rgba(var(--shadow-color),0.16)] max-h-[90vh] overflow-auto w-full {sizeClass}">
			{@render children()}
		</div>
	</div>
{/if}
