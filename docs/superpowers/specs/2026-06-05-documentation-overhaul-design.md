# Documentation & README Overhaul Design

## Overview

Comprehensive documentation overhaul for HyperInfer — a modular Rust & SvelteKit monorepo for building high-performance LLM infrastructure. The overhaul covers three areas: developer documentation site (Zensical), README overhaul across all crates, and a Kubernetes Helm chart.

## Drivers

- **Audience**: Rust developers, Python/ML engineers, DevOps/platform engineers
- **Tone**: Open-source friendly — approachable, quick-start-first, community-oriented
- **Tool**: Zensical (successor to Material for MkDocs, by the same team — actively maintained, Rust+Python)
- **Hosting**: Cloudflare Pages (faster CDN, PR previews, free tier)
- **Logo**: Existing "Electric Jelly" SVG → needs 2 variants (icon-only without background/text, and full logo with "HyperInfer" text)

---

## Architecture / Site Structure

### Repository Layout (new/modified files)

```
docs/
├── index.md                    # Home page — feature overview, badges, quick links
├── get-started.md              # Quick start — pip install, cargo add, docker compose
├── architecture.md             # System architecture — data plane vs control plane
├── guides/                     # Developer guides (primary emphasis)
│   ├── data-plane.md           # Using hyperinfer-client
│   ├── control-plane.md        # Deploying hyperinfer-server
│   ├── routing.md              # Configuring routing strategies
│   ├── providers.md            # Custom providers & registry
│   ├── monitoring.md           # OpenTelemetry & telemetry setup
│   └── python.md               # Python bindings guide
├── reference/                  # Auto-generated API reference
│   └── (cargo doc output, pdoc output)
├── deployment/                 # Operations guides
│   ├── docker.md
│   ├── configuration.md
│   └── kubernetes.md           # References the Helm chart
├── contributing.md
├── mkdocs.yml                  # Zeniscal config
└── assets/
    ├── hyperinfer-icon.svg     # Logo mark (no background, no text)
    └── hyperinfer-logo.svg     # Full logo with "HyperInfer" text
```

### Guides-first approach

Developer guides receive the most attention — they are hand-written, detailed, example-driven. The auto-generated API reference (rustdoc, pdoc) is embedded into the Zensical site as a secondary layer that grows over time.

---

## README Overhaul

### Root README.md

- **Hero section**: Shield badges (GitHub stars, crates.io, PyPI, CI, license, Discord/community)
- **Tagline**: "HyperInfer — The open-source LLM gateway for high-performance AI infrastructure"
- **Architecture diagram** (ASCII art or linked image: Client → Router → Providers)
- **Quick Start** (the killer section):
  - `cargo add hyperinfer-client` / `pip install hyperinfer` / Docker compose up
  - Minimal working example in 3 languages (Rust, Python, curl)
- **Feature grid**: Data Plane, Control Plane, Intelligent Routing, Multi-Provider, Python Bindings, OpenTelemetry
- **Badge links**: to docs site, crates.io, PyPI
- **Contributing / License**

### Crate READMEs

| Crate | Current | Target |
|-------|---------|--------|
| hyperinfer-core | Minimal (7 lines) | Type reference, trait overview |
| hyperinfer-client | Minimal (3 lines) | Usage examples, features, architecture |
| hyperinfer-server | **Missing** | Configuration reference, API overview (auto from utoipa), deployment |
| hyperinfer-providers | Minimal (3 lines) | How to add a custom provider |
| hyperinfer-router | Minimal (3 lines) | Routing strategies explained |
| hyperinfer-python | Brief (9 lines) | Install + basic usage expanded |
| hyperinfer-langchain | Good (45 lines) | Minor polish |
| hyperinfer-llamaindex | Minimal (9 lines) | Expand to match langchain's README |

---

## Zensical Configuration

```yaml
site_name: HyperInfer
site_description: Next-generation LLM Gateway
repo_url: https://github.com/AndroZa0812/HyperInfer
theme:
  name: zensical
  features:
    - navigation.tabs
    - navigation.sections
    - content.code.copy
    - search.suggest
  palette:
    - media: "(prefers-color-scheme: light)"
      scheme: default
    - media: "(prefers-color-scheme: dark)"
      scheme: slate
```

Features enabled: admonitions, content tabs, code copy, search, social cards.

---

## CI & Deployment Pipeline

GitHub Action (`.github/workflows/docs.yml`):
- Trigger: push to `main`, PRs with docs changes
- Steps: install Zensical → build site → deploy to Cloudflare Pages
- Cloudflare Pages PR previews for docs changes in PRs
- **Secrets needed**: `CLOUDFLARE_API_TOKEN` with Cloudflare Pages deployment permissions

---

## Logo Assets

Two SVG files derived from the existing `Electric Jelly.svg`:
1. **hyperinfer-icon.svg** — the jellyfish illustration only (remove dark background rectangle, remove any text paths)
2. **hyperinfer-logo.svg** — icon + "HyperInfer" text rendered as new SVG text element (replacing the original text)

---

## Sidebar: Kubernetes Helm Chart

A separate follow-up task tracked in `.plan/kubernetes-helm-chart.md`:
- `charts/hyperinfer/Chart.yaml` with values, templates (Deployment, Service, Ingress, HPA, ConfigMap, Secret)
- Supports external PostgreSQL/Redis or subchart dependencies
- Health checks, PDB, resource limits
- `helm lint` + `helm template` verification

---

## Scope & Sequencing

1. **Phase 1** — Logo assets (2 SVG variants)
2. **Phase 2** — Root README overhaul with badges + feature grid + quick start
3. **Phase 3** — Crate READMEs (server, core, client, providers, router, python, llamaindex)
4. **Phase 4** — Zensical setup + docs site structure + developer guides
5. **Phase 5** — CI/CD for docs deployment (Cloudflare Pages)
6. **Phase 6** — Helm chart implementation

---

## Acceptance Criteria

- [ ] Root README has shields, architecture diagram, quick-start examples, feature grid
- [ ] Every crate has a meaningful README (no 3-line placeholders)
- [ ] `mkdocs.yml` configures a working Zensical site
- [ ] Development guides exist for data plane, control plane, routing, providers, monitoring, Python
- [ ] Auto-generated API reference (Rust + Python) is embedded
- [ ] CI builds and deploys docs to Cloudflare Pages
- [ ] Helm chart passes `helm lint` and renders valid manifests
- [ ] Logo assets are ready (icon + full logo)
