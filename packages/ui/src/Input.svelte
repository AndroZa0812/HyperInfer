<script lang="ts">
	interface Props {
		label?: string;
		type?: string;
		value?: string | number;
		placeholder?: string;
		disabled?: boolean;
		required?: boolean;
		id?: string;
		name?: string;
		list?: string;
		autocomplete?: 'on' | 'off' | 'email' | 'username' | 'current-password' | 'new-password' | null;
		error?: string;
		class?: string;
		oninput?: (e: Event) => void;
		onblur?: (e: FocusEvent) => void;
	}

	const generatedId = $props.id();

	let {
		label,
		type = 'text',
		value = $bindable(''),
		placeholder,
		disabled = false,
		required = false,
		id = generatedId,
		name,
		list,
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
		{list}
		{type}
		{placeholder}
		{disabled}
		{required}
		{autocomplete}
		bind:value
		{oninput}
		{onblur}
		class="h-10 w-full {type === 'date' ? 'px-3' : 'px-3.5'} text-sm bg-[var(--surface-container-low)] text-[var(--on-surface)] placeholder:text-[var(--outline)] rounded-lg border transition-colors duration-200 outline-none focus:bg-[var(--surface-container-lowest)] {error ? 'border-[var(--error)]' : 'border-[var(--outline-variant)] focus:border-[var(--primary)]'}"
	/>
	{#if error}
		<p class="text-xs text-[var(--error)]">{error}</p>
	{/if}
</div>
