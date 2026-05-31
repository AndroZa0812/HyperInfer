<script lang="ts">
	interface Props {
		variant?: 'success' | 'error' | 'warning' | 'info' | 'neutral';
		label: string;
		class?: string;
	}

	let { variant = 'neutral', label, class: className = '' }: Props = $props();

	const styles = {
		success: 'bg-[var(--success-container)] text-[var(--success)]',
		error: 'bg-[var(--error-container)] text-[var(--error)]',
		warning: 'bg-[var(--warning-container)] text-[var(--warning)]',
		info: 'bg-[var(--primary)]/10 text-[var(--primary)]',
		neutral: 'bg-[var(--surface-container)] text-[var(--on-surface-variant)]'
	};
</script>

<span class="inline-flex items-center px-2.5 py-1 rounded-md text-xs font-medium {styles[variant]} {className}">
	{label}
</span>
