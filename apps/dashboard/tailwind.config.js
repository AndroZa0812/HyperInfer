export default {
    content: ['./src/**/*.{html,js,svelte,ts}'],
    darkMode: 'class',
    theme: {
        extend: {
            colors: {
                primary: 'var(--bg-primary)',
                secondary: 'var(--bg-secondary)',
                accent: 'var(--accent)',
            }
        }
    },
    plugins: []
};