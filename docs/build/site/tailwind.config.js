// Tailwind config for the fyr.guide static site (docs/site).
// This mirrors the inline `tailwind.config` the landing page used with the
// Tailwind Play CDN, so the prebuilt stylesheet renders identically.
const path = require('path');

const SITE = path.resolve(__dirname, '..', '..', 'site');

module.exports = {
  content: [
    path.join(SITE, '**', '*.html').replace(/\\/g, '/'),
    path.join(__dirname, '**', '*.{js,html}').replace(/\\/g, '/'),
  ],
  theme: {
    extend: {
      colors: {
        brand: {
          50: '#fff7ed',
          100: '#ffedd5',
          500: '#f97316',
          600: '#ea580c',
          900: '#7c2d12',
        },
      },
      fontFamily: {
        sans: ['Inter', 'ui-sans-serif', 'system-ui', 'sans-serif'],
        mono: ['Fira Code', 'ui-monospace', 'SFMono-Regular', 'monospace'],
      },
    },
  },
  plugins: [require('@tailwindcss/typography')],
};
