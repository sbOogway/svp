/** Runtime configuration injected by Vite at build time (see .env.example). */
export const env = {
  apiUrl: import.meta.env.VITE_API_URL ?? 'http://localhost:8080',
  wsUrl: import.meta.env.VITE_WS_URL ?? 'ws://localhost:8080/ws',
} as const
