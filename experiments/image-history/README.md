# Image history CLI experiment

Base: `rust-v0.158.0-alpha.2.1` (`0d9c7cbfa6cf1489f55a8a9542b75ddd2c061807`).

This is an experimental Linux x64 CLI for WSL. It does not replace or launch Codex Desktop.
Download the successful GitHub Actions artifact, extract it, and run from WSL:

```sh
sha256sum -c SHA256SUMS
chmod +x bin/codex bin/codex-code-mode-host
bash lab.sh patched login
bash lab.sh patched
```

Login is separate; no desktop credentials are copied. Both launch modes start in a dedicated
empty workspace, with separate `CODEX_HOME` directories under `homes/`. Only use test files.
To compare the unmodified behavior of the same source revision, use `bash lab.sh baseline`.

## Behavior

- The patched launcher enables an 8 MiB aggregate inline-image budget for ordinary sampling requests.
- Most recent images are retained in order. Older images are replaced in the outgoing request
  by notices with local paths. Original bytes are atomically cached under the experimental home.
- The model is explicitly told that omitted pixels are unavailable and to use `view_image`
  when it needs details. Rereading brings that image back as a recent image.
- If the newest image alone exceeds the budget, the request fails with an actionable message.
- Original response-item history is retained, so this bounds ordinary request image bytes,
  **not total disk usage**. Text/tool descriptions also contribute to request size.
- Redundant generated-image event payloads are cleared only in newly persisted copies and
  only if the saved artifact exists. Live events and model-facing originals are preserved.
- Remote compaction, file-backed images, and direct native image-generation response items
  are outside this first experiment's byte cap. This is not a production-complete fix.
- The experiment can reduce prompt-cache reuse when older image entries are replaced.

## Rollback

Exit the experiment (Ctrl+C or `/quit`) and use your usual desktop shortcut or official CLI.
No system environment variable, desktop configuration, official executable, or existing
desktop history is changed. The experimental homes can remain on disk for diagnosis.

This tests CLI behavior only. It does not establish desktop UI compatibility. Do not set
`CODEX_CLI_PATH` to this build for the desktop application.

## Verification

GitHub Actions runs focused Rust tests and a real CLI against a localhost mock Responses server.
The mock uses generated test PNGs, exercises request limits and resume, and needs no OpenAI
credentials. No real screenshots or account files are included in the public build artifacts.
