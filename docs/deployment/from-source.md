# Building & Deploying from Source

> **Status:** describes the Objective05 Rust workspace + Vite/React
> dashboard as it exists today. The packaged-binary / Homebrew /
> AppImage flows documented in `installation.md` will catch up once
> the release pipeline lands.

This guide is the source of truth for:

1. **Prerequisites** — the toolchains required to build, run, test, and
   ship the daemon and the dashboard.
2. **Local development** — running the daemon on your machine and
   pointing the dashboard at it.
3. **Container deployment** — building and running the official Docker
   image.
4. **Production packaging** — what an end-user install bundle looks like
   and how to assemble one.

---

## 1. Prerequisites

| Tool | Version | Used for |
|------|---------|----------|
| Rust | 1.87+ (the repo pins it via `rust-toolchain.toml`) | Daemon + every crate in the workspace |
| Node.js | 20 LTS or newer | Dashboard build / dev server |
| npm | 10+ | Dashboard dependency install |
| make | any | Shortcut targets (`make build`, `make test`, …) |
| Docker | 24+ (optional) | Container build / run |
| `clang` / `pkg-config` | system | Required by some Rust crates (e.g. `kuzu` on Linux) |

Verify your toolchain before doing anything else:

```bash
rustc --version     # ≥ rustc 1.87.0
cargo --version     # ≥ cargo 1.87
node --version      # ≥ v20
npm  --version      # ≥ 10
docker --version    # optional, ≥ 24
```

The `rust-toolchain.toml` at the repo root pins a specific toolchain,
so `cargo` will fetch the right one automatically on first run.

---

## 2. Local Development

### 2.1 Start the Rust daemon

The workspace is bootstrapped with a `Makefile` for the common targets:

```bash
# from the repo root
make build      # cargo build --workspace
make test       # cargo test --workspace
make fmt        # cargo fmt --all
make clippy     # cargo clippy --workspace --all-targets -- -D warnings
make run        # cargo run -p objective -- serve
```

If you prefer to call `cargo` directly:

```bash
# one-time data directory initialization
cargo run -p objective -- setup

# start the API + WebSocket gateway
cargo run -p objective -- serve
```

By default the daemon binds to `127.0.0.1:8080` (REST + OpenAPI) and
`127.0.0.1:8081` (WebSocket). Override via the standard env vars:

```bash
OBJECTIVE__SERVER__REST_PORT=9090 \
OBJECTIVE__SERVER__WEBSOCKET_PORT=9091 \
cargo run -p objective -- serve
```

The first boot initializes `.objective/` in the repo root and seeds
`hackernews_front` + `lobsters` into the `SourceRegistry`. Subsequent
boots read the same on-disk state.

### 2.2 Start the dashboard

The Vite dev server proxies `/api` and `/ws` to the daemon, so the SPA
always talks to `http://127.0.0.1:8080` regardless of where it is
hosted:

```bash
cd dashboard
npm install          # ~250 packages, including vitest + RTL
npm run dev          # http://localhost:5173
```

`npm run dev` boots a Vite server with HMR and the proxy. Open
`http://localhost:5173` to see the UI; it will show
`CONNECTED` in the sidebar footer once the WebSocket handshake
succeeds.

### 2.3 Run the dashboard test suite

```bash
cd dashboard
npm test              # vitest run (18 unit + component tests)
npm run test:watch    # watch mode for development
```

Tests run under `jsdom` and stub the `api` client, so no daemon is
required to exercise the store and component logic.

### 2.4 Build a production dashboard bundle

```bash
cd dashboard
npm run build        # tsc -b && vite build → dist/
```

The `dist/` directory contains a static SPA (~85 kB gzipped JS, ~1.8 kB
gzipped CSS). The Rust gateway can serve it directly from
`objective-gateway`'s `static/` route once the static file serving
hook is wired up in a packaged build.

---

## 3. Container Deployment

The repository ships with a multi-stage `Dockerfile` that produces a
small `debian:bookworm-slim` runtime image:

```Dockerfile
FROM rust:1.87-bookworm AS builder
WORKDIR /app
COPY . .
RUN cargo build --release -p objective

FROM debian:bookworm-slim
RUN useradd --create-home --shell /bin/bash objective
USER objective
WORKDIR /home/objective
COPY --from=builder /app/target/release/objective /usr/local/bin/objective
EXPOSE 8080 8081
CMD ["objective", "serve"]
```

Build and run:

```bash
docker build -t objective:dev .

docker run -d \
  --name objective \
  -p 8080:8080 \
  -p 8081:8081 \
  -v objective-data:/var/lib/objective \
  -v objective-models:/var/lib/objective/models \
  objective:dev
```

Verify the container:

```bash
docker logs objective           # tracing init + boot lines
curl http://localhost:8080/api/v1/health
# → {"status":"healthy", ...}
```

To attach the dashboard:

```bash
docker run -d --name objective-ui \
  -p 5173:5173 \
  -v $(pwd)/dashboard:/app \
  -w /app node:20 \
  sh -c "npm install && npm run dev -- --host 0.0.0.0"
```

…or, for a single-container UX, build the dashboard and bake the
`dist/` output into the daemon image at `/var/lib/objective/ui/` and
point the gateway's static-file route at it.

### Image verification checklist

- `docker run --rm objective:dev objective --version` prints a version
  string.
- `docker run --rm objective:dev objective setup` exits 0 in a throw-away
  container.
- `docker run --rm -p 8080:8080 objective:dev` then
  `curl -fsS http://localhost:8080/api/v1/health` returns
  `{"status":"healthy", ...}` within 5 s.
- `docker run --rm -p 8080:8080 objective:dev` then
  `curl -fsS http://localhost:8080/api/v1/stats` returns a numeric
  document/extraction/event count map.

---

## 4. Production Packaging

Once the release pipeline is finalized, an end-user install bundle
will follow this layout:

```text
objective-{version}-{platform}-{arch}/
├── bin/objective              # daemon binary
├── ui/                        # dashboard static bundle
├── LICENSE-MIT
├── LICENSE-APACHE
├── README.md
└── uninstall.(sh|ps1)
```

The bundle is what Homebrew (`brew install objective`), APT
(`apt install objective`), Winget (`winget install Objective`), and the
direct-download tarballs all wrap.

To assemble a candidate bundle from a fresh checkout:

```bash
# 1. Build the daemon
make build
cargo build --release -p objective

# 2. Build the dashboard
cd dashboard
npm ci
npm run build
cd ..

# 3. Stage the bundle
mkdir -p dist-bundle/bin
mkdir -p dist-bundle/ui
cp target/release/objective dist-bundle/bin/
cp -R dashboard/dist/*    dist-bundle/ui/
cp LICENSE-MIT LICENSE-APACHE README.md dist-bundle/

# 4. (optionally) tar it
tar -czf objective-0.5.0-$(uname -s)-$(uname -m).tar.gz dist-bundle/
```

The packaged binary is expected to call into the `objective-gateway`'s
static-file route to serve the dashboard at `/`, so end users only need
to remember the `http://localhost:8080` URL — no separate dashboard
process to manage.

---

## 5. Observability & Operations

See `observability.md` and `operations.md` in this directory for the
metrics endpoints, log format, recovery service hooks, and the
recommended production hardening checklist (TLS, auth, process
supervision, backup rotation, etc.).

The dashboard surfaces most of the same signals:

- **Header** — live WebSocket status + uptime.
- **Footer** — cumulative ingested / extracted / correlated counts and
  pipeline cycle counter from `/api/v1/monitoring`.
- **Settings → Data & Storage** — live flattened configuration from
  `/api/v1/config` and a one-click JSON export of the entire dataset.
- **Recovery** — `/api/v1/recovery` exposes the watchdog state; the
  POST `/api/v1/recovery/check` endpoint forces a recovery pass.
