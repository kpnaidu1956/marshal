/** @type {import('tailwindcss').Config} */
export default {
  content: [
    "./index.html",
    "./src/**/*.{js,ts,jsx,tsx}",
  ],
  theme: {
    extend: {
      colors: {
        dark: {
          bg: '#0f1117',
          surface: '#1a1d27',
          surface2: '#232733',
          border: '#2d3140',
          text: '#e1e4ed',
          muted: '#8b8fa3',
          accent: '#6c8cff',
          accent2: '#4a6adf',
        },
      },
    },
  },
  plugins: [],
}
