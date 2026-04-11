import { writable } from 'svelte/store';

function createThemeStore() {
    const { subscribe, set, update } = writable<'light' | 'dark'>('light');

    return {
        subscribe,
        toggle: () => {
            update(current => {
                const next = current === 'light' ? 'dark' : 'light';
                localStorage.setItem('theme', next);
                document.documentElement.setAttribute('data-theme', next);
                if (next === 'dark') {
                    document.documentElement.classList.add('dark');
                } else {
                    document.documentElement.classList.remove('dark');
                }
                return next;
            });
        },
        init: () => {
            const stored = localStorage.getItem('theme') as 'light' | 'dark' | null;
            const initial = stored || 'light';
            set(initial);
            document.documentElement.setAttribute('data-theme', initial);
            if (initial === 'dark') {
                document.documentElement.classList.add('dark');
            }
        }
    };
}

export const theme = createThemeStore();
