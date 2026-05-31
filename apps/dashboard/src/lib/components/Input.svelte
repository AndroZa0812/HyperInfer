<script lang="ts">
	interface Props {
		label?: string;
		type?: string;
		value?: string;
		placeholder?: string;
		disabled?: boolean;
		required?: boolean;
		id?: string;
		name?: string;
		autocomplete?: import('svelte/elements').HTMLInputAttributes['autocomplete'];
		error?: string;
		class?: string;
		oninput?: (e: Event) => void;
		onblur?: (e: FocusEvent) => void;
	}

	let {
		label,
		type = 'text',
		value = $bindable(''),
		placeholder,
		disabled = false,
		required = false,
		id,
		name,
		autocomplete,
		error,
		class: className = '',
		oninput,
		onblur
	}: Props = $props();
</script>

<div class="space-y-1.5 {className}">
	{#if label}
		<label
			for={id}
			class="block text-xs font-medium uppercase tracking-wider text-[var(--on-surface-variant)]"
			style="font-family: 'Inter', sans-serif"
		>
			{label}
		</label>
	{/if}
	<input
		{id}
		{name}
		{type}
		{placeholder}
		{disabled}
		{required}
		{autocomplete}
		bind:value
		{oninput}
		{onblur}
		class="w-full px-4 py-3 bg-[var(--surface-container-low)] text-[var(--on-surface)] placeholder:text-[var(--outline)] rounded-lg border-b-2 transition-colors duration-200 outline-none focus:bg-[var(--surface-container)] {error ? 'border-b-[var(--error)]' : 'border-b-[var(--outline-variant)] focus:border-b-[var(--primary)]'}"
	/>
	{#if error}
		<p class="text-xs text-[var(--error)]">{error}</p>
	{/if}
</div>
