import { defineConfig } from 'vite';

export default defineConfig({
  plugins: [
    {
      name: 'force-full-reload',
      handleHotUpdate({ server }) {
        // Broadcast full reload to all connected clients
        server.ws.send({ type: 'full-reload' });
        // Return empty array to tell Vite not to push any hot updates
        return [];
      }
    }
  ]
});
