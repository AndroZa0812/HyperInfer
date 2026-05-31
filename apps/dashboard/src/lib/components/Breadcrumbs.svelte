<script lang="ts">
	interface Crumb {
		label: string;
		href?: string;
	}

	interface Props {
		items: Crumb[];
		class?: string;
	}

	let { items, class: className = '' }: Props = $props();
</script>

<nav class="flex items-center gap-2 text-sm {className}">
	{#each items as item, i}
		{#if i > 0}
			<span class="material-symbols-outlined text-[var(--outline)]" style="font-size: 18px">chevron_right</span>
		{/if}
		{#if item.href && i < items.length - 1}
			<a href={item.href} class="text-[var(--on-surface-variant)] hover:text-[var(--primary)] transition-colors">
				{item.label}
			</a>
		{:else}
			<span class="text-[var(--on-surface)] font-medium">{item.label}</span>
		{/if}
	{/each}
</nav>
