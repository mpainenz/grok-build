# ACP Integration Map — grok agent → TensorRelay Agent surface

How the Tauri app drives `tensorrelay-agent` (this fork's `grok agent stdio`)
over the Agent Client Protocol, and how each ACP concept maps onto ADR-0030's
UI. Verified 2026-07-17 with `acp-probe/` against the real binary.

## Transport
- `grok agent stdio` speaks **JSON-RPC 2.0, newline-delimited**, over the
  child's stdin/stdout. The published `agent-client-protocol` crate (**0.10.4**,
  same version the fork uses) does its own framing — `ClientSideConnection::new`
  takes raw `AsyncRead`/`AsyncWrite`; grok's internal `LineBufferedRead` is only
  a perf wrapper we don't need.
- The app is the **client** (implements `acp::Client`); the agent binary is the
  **agent** (implements `acp::Agent`). App calls `initialize / authenticate /
  new_session / prompt / cancel`; agent calls back `request_permission /
  session_notification / read_text_file / write_text_file / …`.
- Supervision mirrors `runtime_process.rs` (the pattern that supervises
  `tensorrelay-runtime`): spawn, own stdin/stdout, capture stderr, restart on
  crash. One agent process per app is enough — sessions multiplex over it.

## Verified handshake (`acp-probe initialize`, no cluster required)
```
protocolVersion: 1
agentCapabilities:
  loadSession: true                 # harness can resume a session by id
  promptCapabilities: image=false, audio=false, embeddedContext=true
  mcpCapabilities: http=true, sse=true
  _meta:
    x.ai/fs_notify: true
    x.ai/hooks: blockingEvents=[pre_tool_use], decisions=[deny]
authMethods: ["xai.api_key", "grok.com"]
```
Consequences:
- `loadSession: true` confirms the **resume cache** half of the Agent Session
  Store decision is real — the harness can reopen a session; our app-owned
  event log stays the display/search/retention truth (ADR-0030).
- `x.ai/hooks` blocking `pre_tool_use` with a `deny` decision is a **second**
  gate besides `request_permission` — useful for hard, non-interactive policy
  (e.g. a schedule's fail-closed shell/MCP), distinct from the interactive
  approval that Manual mode surfaces.
- `authMethods` are enumerated in-band; the app never renders them for local
  mode. The login gate must be excised for local-endpoint use (kill-list #1);
  a dummy `XAI_API_KEY` is the interim bypass.

## Seam → UI mapping (the two required `Client` methods)
| ACP callback | ADR-0030 surface |
|---|---|
| `request_permission(options)` | **Session Mode.** Manual approval → render the approval card (Allow once / Allow for this session / Deny map to the option kinds; `AllowOnce`/`AllowAlways`/`RejectOnce`). Auto-run → auto-select `AllowOnce`. Per-session "allow for this session" memory lives app-side, keyed by the tool/command pattern. |
| `session_notification(update)` | **Transcript event stream**, appended to the Agent Session Store as it renders. Variants: `AgentMessageChunk` = streamed assistant text; `AgentThoughtChunk` = reasoning; `ToolCall` / `ToolCallUpdate` = the tool cards (and their auto-approved/approved/denied state); `Plan` = plan view; `UserMessageChunk` = echoed input. |
| `read_text_file` / `write_text_file` | **Workspace confinement.** The app implements these against the picked Workspace and **rejects paths outside it** — this is where client-side confinement is enforced, not in the agent. Declared via `ClientCapabilities.fs`. |
| `create_terminal` / `terminal_output` / … | Declared `terminal(false)` for v1 (shell runs via the agent's own tools under `request_permission`, not a client-hosted PTY). Revisit if we want live terminal panes. |

## App-call → UI mapping (the `Agent` methods we invoke)
| App call | Trigger |
|---|---|
| `initialize` | Once per agent process at supervision start. |
| `authenticate` | Local mode: skipped/dummied (kill-list #1). |
| `new_session(cwd, mcp_servers)` | Opening/creating a Session in a Workspace; `cwd` = the Workspace (or its worktree for an Isolated Session); `mcp_servers` = user-configured MCP (incl. the shipped search server when enabled). |
| `prompt(session_id, blocks)` | Sending a message. While a turn runs, further sends **queue** app-side (ADR-0030 queue-while-running); the activity pulse tracks the open turn. |
| `cancel` | The Stop control. |
| `load_session` | Reopening a persisted Session for resume (capability confirmed). |

## Next slice (Phase 2, in the app)
1. New `agent` module in `client/src-tauri` (its own file set, to avoid churn in
   the shared crate) supervising one `tensorrelay-agent` process — the
   `runtime_process.rs` shape.
2. Port `acp::Client` from `acp-probe` into it, backing `request_permission`
   with the Session Mode state and `session_notification` with the event store.
3. Tauri commands/events bridging ACP ↔ the React Agent page (replacing the
   lab's static fixtures with live session events).
Land this after the concurrent catalog WIP settles, to avoid tree collisions.
