const express = require("express");
const path = require("path");
const { createServer: createViteServer } = require("vite");

async function startServer() {
  const app = express();

  // ΑΥΤΟ ΕΙΝΑΙ ΤΟ ΚΛΕΙΔΙ: Τα headers που ζητάει ο browser
  app.use((req, res, next) => {
    res.setHeader("Cross-Origin-Opener-Policy", "same-origin");
    res.setHeader("Cross-Origin-Embedder-Policy", "require-corp");
    next();
  });

  // Ειδικό route για το WASM kernel
  app.use(
    "/wasm",
    express.static(
      path.join(__dirname, "kernel/target/wasm32-unknown-unknown/release"),
    )
  );

  // Ενσωμάτωση του Vite Middleware
  const vite = await createViteServer({
    server: { middlewareMode: true },
    appType: 'spa'
  });
  app.use(vite.middlewares);

  app.listen(3000, () => {
    console.log("OS Kernel Server running at http://localhost:3000");
  });
}

startServer();
