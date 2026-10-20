# Portal

Management portal foundation built with React, Vite, TypeScript, Tailwind CSS
and shadcn/ui. Run commands from the repository root:

```sh
pnpm --filter @backend-tracker/portal dev
pnpm --filter @backend-tracker/portal typecheck
pnpm --filter @backend-tracker/portal lint
pnpm --filter @backend-tracker/portal test
pnpm --filter @backend-tracker/portal build
```

The Vite dev server proxies `/v1` HTTP and WebSocket traffic to
`http://127.0.0.1:8080` by default. Set `VITE_API_PROXY_TARGET` in a local
environment file to change the backend origin. The proxy preserves backend
responses, including `/v1` errors; it does not route API paths to the SPA.

This scaffold does not implement login, organization membership, device
provisioning, maps, history or tracking data. TanStack Query, React Hook Form
and Zod are installed for the planned API and form work; no sensitive state is
persisted by this shell.
