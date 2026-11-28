<script lang="ts">
	export interface InputSuggestion {
		value: string;
		label?: string;
	}

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
		suggestions?: InputSuggestion[];
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
		suggestions,
		autocomplete,
		error,
		class: className = '',
		oninput,
		onblur
	}: Props = $props();

	const MAX_VISIBLE_SUGGESTIONS = 60;

	let open = $state(false);
	let typingFilter = $state(false);
	let highlightedIndex = $state(-1);
	let wrapperEl = $state<HTMLDivElement | null>(null);
	let inputEl = $state<HTMLInputElement | null>(null);
	let menuEl = $state<HTMLDivElement | null>(null);
	let menuStyle = $state('');

	let hasSuggestions = $derived(Boolean(suggestions && suggestions.length > 0));

	let filteredSuggestions = $derived.by(() => {
		if (!suggestions || suggestions.length === 0) return [];
		if (!typingFilter) return suggestions;
		const q = String(value ?? '').trim().toLowerCase();
		if (!q) return suggestions;
		return suggestions.filter(
			(s) =>
				s.value.toLowerCase().includes(q) ||
				(s.label ? s.label.toLowerCase().includes(q) : false)
		);
	});

	let visibleSuggestions = $derived(filteredSuggestions.slice(0, MAX_VISIBLE_SUGGESTIONS));

	function portal(node: HTMLElement) {
		document.body.appendChild(node);
		return {
			destroy() {
				node.remove();
			}
		};
	}

	function updatePosition() {
		if (!inputEl || typeof window === 'undefined') return;
		const rect = inputEl.getBoundingClientRect();
		const spaceBelow = window.innerHeight - rect.bottom - 12;
		const spaceAbove = rect.top - 12;
		const flipUp = spaceBelow < 180 && spaceAbove > spaceBelow;
		const maxHeight = Math.max(120, Math.min(260, flipUp ? spaceAbove : spaceBelow));
		const width = Math.min(Math.max(rect.width, 180), window.innerWidth - 16);
		const left = Math.max(8, Math.min(rect.left, window.innerWidth - width - 8));

		if (flipUp) {
			const bottom = window.innerHeight - rect.top + 4;
			menuStyle = `position:fixed;bottom:${Math.round(bottom)}px;left:${Math.round(left)}px;width:${Math.round(width)}px;max-height:${Math.round(maxHeight)}px;z-index:9999;`;
		} else {
			const top = rect.bottom + 4;
			menuStyle = `position:fixed;top:${Math.round(top)}px;left:${Math.round(left)}px;width:${Math.round(width)}px;max-height:${Math.round(maxHeight)}px;z-index:9999;`;
		}
	}

	function openSuggestions(filterByText: boolean) {
		if (disabled || !hasSuggestions) return;
		typingFilter = filterByText;
		highlightedIndex = -1;
		updatePosition();
		open = true;
	}

	function pickSuggestion(item: InputSuggestion) {
		value = item.value;
		open = false;
		typingFilter = false;
		inputEl?.focus();
	}

	function handleInput(e: Event) {
		oninput?.(e);
		if (hasSuggestions) {
			typingFilter = true;
			highlightedIndex = 0;
			updatePosition();
			open = true;
		}
	}

	function handleFocus() {
		if (hasSuggestions) {
			openSuggestions(false);
		}
	}

	function handleBlur(e: FocusEvent) {
		const next = e.relatedTarget as Node | null;
		if (next && menuEl?.contains(next)) {
			return;
		}
		open = false;
		onblur?.(e);
	}

	function handleKeydown(e: KeyboardEvent) {
		if (!hasSuggestions) return;
		if (e.key === 'ArrowDown') {
			e.preventDefault();
			if (!open) {
				openSuggestions(false);
				highlightedIndex = 0;
			} else if (visibleSuggestions.length > 0) {
				highlightedIndex = (highlightedIndex + 1) % visibleSuggestions.length;
				menuEl?.querySelector<HTMLElement>(`[data-sug-idx="${highlightedIndex}"]`)?.scrollIntoView({ block: 'nearest' });
			}
		} else if (e.key === 'ArrowUp' && open && visibleSuggestions.length > 0) {
			e.preventDefault();
			highlightedIndex = (highlightedIndex - 1 + visibleSuggestions.length) % visibleSuggestions.length;
			menuEl?.querySelector<HTMLElement>(`[data-sug-idx="${highlightedIndex}"]`)?.scrollIntoView({ block: 'nearest' });
		} else if (e.key === 'Enter' && open && highlightedIndex >= 0 && visibleSuggestions[highlightedIndex]) {
			e.preventDefault();
			pickSuggestion(visibleSuggestions[highlightedIndex]);
		} else if (e.key === 'Tab' && open) {
			open = false;
		}
	}

	$effect(() => {
		if (!open || typeof window === 'undefined') return;
		updatePosition();

		const onReposition = () => updatePosition();
		const onPointerDown = (e: PointerEvent) => {
			const target = e.target as Node | null;
			if (target && (wrapperEl?.contains(target) || menuEl?.contains(target))) {
				return;
			}
			open = false;
		};
		const onWindowKeydown = (e: KeyboardEvent) => {
			if (e.key === 'Escape') {
				e.stopPropagation();
				e.preventDefault();
				open = false;
			}
		};

		window.addEventListener('scroll', onReposition, { capture: true, passive: true });
		window.addEventListener('resize', onReposition, { passive: true });
		window.addEventListener('pointerdown', onPointerDown, { capture: true });
		window.addEventListener('keydown', onWindowKeydown, { capture: true });

		return () => {
			window.removeEventListener('scroll', onReposition, { capture: true });
			window.removeEventListener('resize', onReposition);
			window.removeEventListener('pointerdown', onPointerDown, { capture: true });
			window.removeEventListener('keydown', onWindowKeydown, { capture: true });
		};
	});
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
	<div bind:this={wrapperEl} class="relative">
		<input
			bind:this={inputEl}
			{id}
			{name}
			list={hasSuggestions ? undefined : list}
			{type}
			{placeholder}
			{disabled}
			{required}
			autocomplete={hasSuggestions ? 'off' : autocomplete}
			bind:value
			oninput={handleInput}
			onclick={() => { if (hasSuggestions && !open) openSuggestions(false); }}
			onfocus={handleFocus}
			onblur={handleBlur}
			onkeydown={handleKeydown}
			class="h-10 w-full {type === 'date' ? 'px-3' : hasSuggestions ? 'pl-3.5 pr-8' : 'px-3.5'} text-sm bg-[var(--surface-container-low)] text-[var(--on-surface)] placeholder:text-[var(--outline)] rounded-lg border transition-colors duration-200 outline-none focus:bg-[var(--surface-container-lowest)] {error ? 'border-[var(--error)]' : 'border-[var(--outline-variant)] focus:border-[var(--primary)]'}"
		/>
		{#if hasSuggestions}
			<button
				type="button"
				tabindex="-1"
				aria-label="Toggle suggestions"
				{disabled}
				onmousedown={(e) => {
					e.preventDefault();
					if (open) open = false;
					else {
						inputEl?.focus();
						openSuggestions(false);
					}
				}}
				class="absolute right-2 top-1/2 -translate-y-1/2 p-0.5 text-[var(--on-surface-variant)] hover:text-[var(--on-surface)] cursor-pointer"
			>
				<span
					aria-hidden="true"
					class="material-symbols-outlined transition-transform duration-150 {open ? 'rotate-180' : ''}"
					style="font-size: 18px"
				>
					expand_more
				</span>
			</button>
		{/if}
	</div>
	{#if error}
		<p class="text-xs text-[var(--error)]">{error}</p>
	{/if}
</div>

{#if open && visibleSuggestions.length > 0}
	<div
		use:portal
		bind:this={menuEl}
		role="listbox"
		tabindex="-1"
		style={menuStyle}
		class="overflow-y-auto rounded-lg bg-[var(--surface-container-lowest)] border border-[var(--ghost-border)] shadow-[0_12px_32px_rgba(var(--shadow-color),0.18)] py-1"
	>
		{#each visibleSuggestions as item, idx}
			{@const selected = String(value ?? '') === item.value}
			{@const highlighted = idx === highlightedIndex}
			<button
				type="button"
				role="option"
				data-sug-idx={idx}
				aria-selected={selected}
				onmouseenter={() => { highlightedIndex = idx; }}
				onmousedown={(e) => {
					e.preventDefault();
					pickSuggestion(item);
				}}
				class="w-full px-3 py-2 text-left text-sm flex items-center justify-between gap-3 transition-colors cursor-pointer {selected
					? 'text-[var(--primary)] font-medium bg-[var(--primary)]/10'
					: highlighted
						? 'bg-[var(--surface-container)] text-[var(--on-surface)]'
						: 'text-[var(--on-surface)] hover:bg-[var(--surface-container)]'}"
			>
				<span class="font-mono text-xs truncate">{item.value}</span>
				{#if item.label && item.label !== item.value}
					<span class="text-xs text-[var(--on-surface-variant)] truncate max-w-[45%]">{item.label}</span>
				{/if}
			</button>
		{/each}
		{#if filteredSuggestions.length > MAX_VISIBLE_SUGGESTIONS}
			<div class="px-3 py-1.5 text-[11px] text-[var(--on-surface-variant)] border-t border-[var(--ghost-border)]">
				Showing {MAX_VISIBLE_SUGGESTIONS} of {filteredSuggestions.length.toLocaleString()} — type to narrow
			</div>
		{/if}
	</div>
{/if}
