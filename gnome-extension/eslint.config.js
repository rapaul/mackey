// Minimal flat-config lint for the GNOME Shell extension. GJS exposes `global`
// and `console` as ambient globals and loads modules as ESM via `gi://` /
// `resource://` specifiers. `gnome-extensions pack` only bundles extension.js
// and metadata.json, so this file is not shipped in the .shell-extension.zip.
export default [
    {
        files: ['extension.js'],
        languageOptions: {
            ecmaVersion: 2022,
            sourceType: 'module',
            globals: {
                global: 'readonly',
                console: 'readonly',
                globalThis: 'readonly',
            },
        },
        rules: {
            'no-unused-vars': 'error',
            'no-undef': 'error',
            eqeqeq: 'error',
        },
    },
];
