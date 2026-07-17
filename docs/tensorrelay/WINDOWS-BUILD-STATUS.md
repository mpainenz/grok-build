# Windows agent-release build — known blocker

**Status (2026-07-17):** Linux builds green and ships. **Windows fails**, so the
`tensorrelay-agent-windows-x64.zip` asset is absent and the tensorrelay Steam
build falls back loudly (Windows users get the gated Agent page until this is
fixed). Linux is unaffected.

## Fixed along the way
1. `protoc` missing → `arduino/setup-protoc` on both runners.
2. `$PROTOC` set to a git-bash unix path that `PathBuf::from` can't resolve on
   Windows → `cygpath -w` to a native path. Confirmed in logs:
   `PROTOC: C:\hostedtoolcache\windows\protoc\v23.4\x64\bin\protoc.exe`.
3. Two build scripts (`xai-grok-tools`, `xai-grok-shell`) download ripgrep at
   build time → `GROK_{TOOLS,SHELL}_BUNDLE_RG_PATH` point at the runner's rg.

## The remaining failure
`xai-grok-tools-api/build.rs:33` — the `compile_protos(...).unwrap()` — panics
with **`protoc command failed`**. So protoc is now *found* (env is correct) but
its invocation returns non-zero. The identical protoc (v23.4, setup-protoc) and
the same code compile fine on Linux, so this is Windows-environment-specific,
not a proto-content problem.

## Next steps (needs a Windows machine or one diagnostic CI cycle)
- Capture protoc's own stderr: prost/`xai-proto-build` swallows it into the
  terse "protoc command failed". Add a temporary step running the exact
  `protoc --proto_path=proto proto/grok-tools.proto -o /dev/null` (or the
  descriptor set the build uses) on the Windows runner to see the real error.
- Suspects: include-dir resolution (`../include` next to protoc.exe) with
  backslash paths, or a working-directory/relative-path assumption in
  `xai-proto-build` that only bites on Windows.
- Once green at a commit, bump `client/agent-harness.version` in tensorrelay to
  that commit so the next Steam build bundles both platforms.
