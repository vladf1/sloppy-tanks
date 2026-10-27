import type { Connect, Plugin } from "vite";

/**
 * Vite's dev and preview servers serve the page only under `base` with its trailing
 * slash, and their hint for `/sloppy-tanks?map=harbor` appends the query to the path
 * (`/sloppy-tanks/sloppy-tanks?map=harbor`). Redirect the bare base, query and all,
 * to the page instead.
 */
export function baseRedirect(base: string): Plugin {
  const middleware = baseRedirectMiddleware(base);
  return {
    name: "base-redirect",
    // Added directly, not from a returned hook, so it runs before Vite's base check.
    configureServer: (server) => void server.middlewares.use(middleware),
    configurePreviewServer: (server) => void server.middlewares.use(middleware),
  };
}

export function baseRedirectMiddleware(base: string): Connect.NextHandleFunction {
  const bare = base.replace(/\/$/, "");
  return (request, response, next) => {
    const url = request.url ?? "";
    const query = url.indexOf("?");
    const path = query < 0 ? url : url.slice(0, query);
    if (!bare || path !== bare) return next();
    response.writeHead(302, { Location: base + (query < 0 ? "" : url.slice(query)) });
    response.end();
  };
}
