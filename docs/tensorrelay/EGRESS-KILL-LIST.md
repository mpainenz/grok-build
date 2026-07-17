# TensorRelay Egress Kill-List

This fork ships as `tensorrelay-agent` inside TensorRelay (ADR-0030 in the
tensorrelay repo). Its egress policy is strict: **loopback (the Node-Local
Inference API) plus user-configured MCP servers — nothing else.** Every
xAI-bound or third-party-bound surface below must be excised or hard-disabled,
and this list is re-audited on every upstream pull (grep the Audit column's
patterns; anything new gets a row before the merge lands).

Status: `identified` → `disabled` (compiled in, unreachable, returns error) →
`removed` (code deleted). Target for v1 ship is at least `disabled` on every
row, `removed` where cheap.

| # | Surface | Where (first pass, 8adf901) | Audit pattern | Status |
|---|---------|------------------------------|---------------|--------|
| 1 | xAI login/OAuth (device flow, sign-in) | `accounts.x.ai`, `auth.x.ai` refs across auth crates | `accounts\.x\.ai\|auth\.x\.ai` | identified |
| 2 | Default model API fallback | `api.x.ai/v1` defaults (client, models) | `api\.x\.ai` | identified — config `base_url` bypasses it, but the fallback must not silently engage when config is absent |
| 3 | GCS session/search remote sync (the upload-scandal remnant) | `xai-grok-shell/src/session/storage/search_remote_sync.rs` | `storage\.googleapis` | identified |
| 4 | Update checks via GCS | `xai-grok-update/src/version.rs` | `storage\.googleapis\|xai-grok-update` | identified |
| 5 | Telemetry exporters (OTLP http/grpc), unified log upload | `common/xai-tracing/src/{http_client,grpc_client}.rs` | `xai-tracing` exporters | identified |
| 6 | Metric donation | `common/xai-computer-hub-sdk/src/metric_donate.rs` | `metric_donate` | identified |
| 7 | Announcements fetch | `xai-grok-announcements` crate + welcome top bar | `announcements` | identified |
| 8 | Plugin marketplace/registry fetch + install | `xai-grok-agent/src/plugins/{marketplace,registry,install_registry,manifest}.rs` | `marketplace\|install_registry` | identified |
| 9 | Model-backend web search | `supports_backend_search`, `web_search` sampler → xAI search model | `backend_search\|web_search` | identified — dies naturally with the xAI cut; TensorRelay ships an MCP search server instead (off by default) |
| 10 | Speech-to-text | `api.x.ai/v1/stt` ref | `/v1/stt` | identified |

Permitted egress (the allowlist, not part of the kill-list):
- `http://127.0.0.1:<port>/v1` — the TensorRelay Node-Local Inference API
  (config `base_url`; keyless on loopback).
- MCP servers the user explicitly configures (stdio or local/remote per user
  choice; TensorRelay's shipped web-search MCP server is off by default and
  privacy-disclosed on enable).

Fork conventions:
- TensorRelay-specific files live under `docs/tensorrelay/` and (later)
  behind a `tensorrelay` cargo feature, to keep upstream rebases cheap.
- Upstream remote: `xai-org/grok-build`; origin: `mpainenz/grok-build`
  (same convention as the llama.cpp fork).
