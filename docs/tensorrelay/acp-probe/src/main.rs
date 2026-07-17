//! ACP seam probe for TensorRelay's Agent surface (tensorrelay ADR-0030).
//!
//! Spawns `grok agent stdio` and drives it over the Agent Client Protocol the
//! same way the Tauri app will: a `ClientSideConnection` whose `Client` impl is
//! the two seam points that map onto our UI —
//!   - `request_permission`  ->  Session Mode (Manual approval prompts / Auto-run
//!                                auto-approves). Here we auto-approve and log.
//!   - `session_notification` ->  the transcript event stream (AgentMessageChunk
//!                                is the streamed assistant text; tool calls,
//!                                plans, etc. arrive as other SessionUpdate
//!                                variants). Here we print each update.
//!
//! Usage:
//!   GROK_BIN=/path/to/xai-grok-pager \
//!   GROK_HOME=/tmp/grok-smoke \
//!   XAI_API_KEY=dummy-local \
//!   tr-acp-probe [initialize|prompt "text"]
//!
//! `initialize` (default) needs no cluster — the handshake precedes inference.
//! `prompt` additionally authenticates, opens a session, and streams a reply;
//! it needs a Serving cluster on the Node-Local Inference API.

use std::cell::Cell;
use std::process::Stdio;
use std::rc::Rc;

use agent_client_protocol::{self as acp, Agent as _};
use anyhow::{Context, Result};
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

struct ProbeClient {
    permission_requests: Rc<Cell<u32>>,
    notifications: Rc<Cell<u32>>,
}

#[async_trait::async_trait(?Send)]
impl acp::Client for ProbeClient {
    async fn request_permission(
        &self,
        args: acp::RequestPermissionRequest,
    ) -> acp::Result<acp::RequestPermissionResponse> {
        // Auto-run behaviour: pick AllowOnce if offered, else the first option.
        // In the app this is exactly the fork: Manual mode surfaces the
        // approval card and awaits the user; Auto-run does this automatically.
        self.permission_requests.set(self.permission_requests.get() + 1);
        let opt = args
            .options
            .iter()
            .find(|o| o.kind == acp::PermissionOptionKind::AllowOnce)
            .or_else(|| args.options.first());
        let ids: Vec<&str> = args.options.iter().map(|o| &*o.option_id.0).collect();
        eprintln!("  [permission] tool wants to run; options={ids:?} -> auto-allow");
        let outcome = match opt {
            Some(o) => acp::RequestPermissionOutcome::Selected(
                acp::SelectedPermissionOutcome::new(o.option_id.clone()),
            ),
            None => acp::RequestPermissionOutcome::Cancelled,
        };
        Ok(acp::RequestPermissionResponse::new(outcome))
    }

    async fn session_notification(&self, args: acp::SessionNotification) -> acp::Result<()> {
        self.notifications.set(self.notifications.get() + 1);
        match args.update {
            acp::SessionUpdate::AgentMessageChunk(acp::ContentChunk { content, .. }) => {
                if let acp::ContentBlock::Text(t) = content {
                    print!("{}", t.text);
                    use std::io::Write;
                    let _ = std::io::stdout().flush();
                }
            }
            other => {
                // Tool calls, plans, thoughts, tool-call updates: the variants
                // the transcript renders as cards. Log their kind.
                eprintln!("  [update] {}", update_kind(&other));
            }
        }
        Ok(())
    }
}

fn update_kind(u: &acp::SessionUpdate) -> &'static str {
    match u {
        acp::SessionUpdate::AgentMessageChunk(_) => "agent_message_chunk",
        acp::SessionUpdate::AgentThoughtChunk(_) => "agent_thought_chunk",
        acp::SessionUpdate::ToolCall(_) => "tool_call",
        acp::SessionUpdate::ToolCallUpdate(_) => "tool_call_update",
        acp::SessionUpdate::UserMessageChunk(_) => "user_message_chunk",
        acp::SessionUpdate::Plan(_) => "plan",
        _ => "other",
    }
}

async fn run() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let mode = args.next().unwrap_or_else(|| "initialize".to_string());
    let prompt_text = args.next().unwrap_or_else(|| "Say hello in three words.".to_string());

    let bin = std::env::var("GROK_BIN")
        .context("set GROK_BIN to the built xai-grok-pager binary")?;

    let mut cmd = tokio::process::Command::new(&bin);
    cmd.args(["agent", "stdio"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());
    // Local-endpoint smoke env (see EGRESS-KILL-LIST.md #1, #2b): a dummy key
    // satisfies the login gate, and the catalog prefetch is redirected off
    // api.x.ai onto loopback.
    if std::env::var("XAI_API_KEY").is_err() {
        cmd.env("XAI_API_KEY", "dummy-local");
    }
    if std::env::var("GROK_MODELS_BASE_URL").is_err() {
        cmd.env("GROK_MODELS_BASE_URL", "http://127.0.0.1:8677/v1");
    }

    let mut child = cmd.spawn().context("spawn grok agent stdio")?;
    let outgoing = child.stdin.take().unwrap().compat_write();
    let incoming = child.stdout.take().unwrap().compat();

    let permission_requests = Rc::new(Cell::new(0u32));
    let notifications = Rc::new(Cell::new(0u32));
    let client = ProbeClient {
        permission_requests: permission_requests.clone(),
        notifications: notifications.clone(),
    };

    let (conn, io_task) = acp::ClientSideConnection::new(client, outgoing, incoming, |fut| {
        tokio::task::spawn_local(fut);
    });
    tokio::task::spawn_local(io_task);

    eprintln!("== initialize ==");
    let init = conn
        .initialize(
            acp::InitializeRequest::new(acp::ProtocolVersion::V1).client_capabilities(
                acp::ClientCapabilities::new()
                    .fs(acp::FileSystemCapabilities::new())
                    .terminal(false),
            ),
        )
        .await
        .context("initialize")?;

    println!("protocolVersion: {:?}", init.protocol_version);
    println!("agentCapabilities: {}", serde_json::to_string_pretty(&init.agent_capabilities)?);
    let auth_ids: Vec<String> = init.auth_methods.iter().map(|m| m.id().0.to_string()).collect();
    println!("authMethods: {auth_ids:?}");

    if mode == "prompt" {
        eprintln!("== authenticate ==");
        if let Some(m) = init.auth_methods.iter().find(|m| &*m.id().0 == "xai.api_key") {
            conn.authenticate(acp::AuthenticateRequest::new(m.id().clone()))
                .await
                .context("authenticate")?;
        }
        eprintln!("== new_session ==");
        let cwd = std::env::current_dir()?;
        let sess = conn
            .new_session(acp::NewSessionRequest::new(cwd).mcp_servers(vec![]))
            .await
            .context("new_session")?;
        eprintln!("== prompt (streaming reply below) ==");
        let resp = conn
            .prompt(acp::PromptRequest::new(
                sess.session_id.clone(),
                vec![acp::ContentBlock::Text(acp::TextContent::new(prompt_text))],
            ))
            .await
            .context("prompt")?;
        println!("\n-- stopReason: {:?}", resp.stop_reason);
    }

    println!(
        "\nseam counters: permission_requests={}, notifications={}",
        permission_requests.get(),
        notifications.get()
    );
    let _ = child.start_kill();
    Ok(())
}

fn main() -> Result<()> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let local = tokio::task::LocalSet::new();
    local.block_on(&rt, run())
}
