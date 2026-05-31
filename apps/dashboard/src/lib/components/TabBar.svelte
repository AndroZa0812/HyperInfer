<script lang="ts">
	interface Props {
		items: { label: string; value: string }[];
		active?: string;
		onchange?: (value: string) => void;
		class?: string;
	}

	let {
		items,
		active = $bindable(''),
		onchange,
		class: className = ''
	}: Props = $props();

	function select(value: string) {
		active = value;
		onchange?.(value);
	}
</script>

<div class="flex gap-1 {className}">
	{#each items as item}
		<button
			class="px-4 py-2 text-sm font-medium rounded-lg transition-all duration-200 cursor-pointer {active === item.value
				? 'text-[var(--primary)] bg-[var(--primary)]/10'
				: 'text-[var(--on-surface-variant)] hover:text-[var(--on-surface)] hover:bg-[var(--surface-container)]'}"
			onclick={() => select(item.value)}
		>
			{item.label}
		</button>
	{/each}
</div>
