/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./src/**/*.{ts,tsx}"],
  theme: {
    extend: {
      animation: {
        "panel-in": "panelIn 420ms cubic-bezier(0.21, 0.9, 0.24, 1) both",
        "toast-out": "toastOut 200ms ease forwards",
      },
      keyframes: {
        panelIn: {
          "0%": { opacity: "0", transform: "translateY(12px) scale(0.99)" },
          "100%": { opacity: "1", transform: "translateY(0) scale(1)" },
        },
        toastOut: {
          "0%": { opacity: "1", transform: "scale(1)" },
          "100%": { opacity: "0", transform: "scale(0.96)" },
        },
      },
    },
  },
  plugins: [],
};
