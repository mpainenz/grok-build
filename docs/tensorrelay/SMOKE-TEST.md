# Smoke test: stock grok TUI against a TensorRelay cluster

The cheapest end-to-end proof of ADR-0030's thesis: the unmodified harness,
driving inference through the Node-Local Inference API. No fork surgery
required — just config.

## Prerequisites
- A TensorRelay cluster **Serving** a tool-capable model (any current catalog
  model; `curl http://127.0.0.1:8677/v1/models` answers).
- This repo built: `cargo build --release -p xai-grok-pager-bin`
  (binary: `target/release/xai-grok-pager`).

## Run
```sh
mkdir -p /tmp/grok-smoke
cp docs/tensorrelay/smoke-config.toml /tmp/grok-smoke/config.toml
GROK_HOME=/tmp/grok-smoke ./target/release/xai-grok-pager
```

## What to verify
1. Startup reaches the workspace picker / prompt without demanding xAI login
   (local models need no key; if it forces auth, that's kill-list row 1
   biting and must be noted).
2. A trivial prompt round-trips: streamed tokens arrive from the cluster
   (`reasoning_content` deltas are fine to see with Qwen).
3. A tool-using prompt ("list the files in this directory, then read one")
   produces native tool_calls — the runtime streams `tool_calls` deltas
   (ADR-0029) and the harness executes them.
4. `usage` shows in the transcript/token counter (the harness requests
   `stream_options.include_usage`).
5. Nothing leaves the machine except 127.0.0.1: watch with
   `sudo tcpdump -i any -n 'not host 127.0.0.1'` for the session, or at
   least check no auth/announcement fetch errors in the log
   (`RUST_LOG=debug GROK_LOG_FILE=/tmp/grok-smoke/grok.log`).

## Known knobs
- `context_window` must be set explicitly (grok defaults to 200k; catalog
  models below frontier will overrun). The fork will later auto-set it from
  `/status` — until then the config carries it.
- `stream_tool_calls` stays UNSET (xAI extension; upstream docs warn it
  breaks OpenAI-compatible providers).
