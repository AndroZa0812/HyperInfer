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

<div class="inline-flex bg-[var(--surface-container-low)] rounded-lg p-1 gap-1 {className}">
	{#each items as item}
		<button
			class="px-4 py-2 text-sm font-medium rounded-md transition-all duration-200 cursor-pointer {active === item.value
				? 'bg-[var(--surface-container-lowest)] text-[var(--primary)] shadow-sm'
				: 'text-[var(--on-surface-variant)] hover:text-[var(--on-surface)]'}"
			onclick={() => select(item.value)}
		>
			{item.label}
		</button>
	{/each}
</div>
