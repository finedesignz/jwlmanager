# Windows Authenticode signing (Azure Trusted Signing)

This directory holds the JWL Manager Windows signing wiring: a script Tauri
invokes through its bundler's own sign-command hook, and the metadata that
script needs. Both files are committed but INERT -- they do nothing on a
local build, a dev build, or a pull-request build. Only the tag-triggered
release workflow (`.github/workflows/release-app.yml`), and only when the
repository enable variable is turned on, ever invokes `sign.ps1`.

## What is here

- `sign.ps1` -- invoked by Tauri's `bundle.windows.signCommand` during
  bundling, once the release workflow injects that entry into the CI
  workspace's copy of `tauri.conf.json`. It signs one artifact per
  invocation (the path Tauri passes as `%1`), using `signtool.exe` plus the
  Azure Trusted Signing dlib. It refuses to run -- non-zero exit, explanatory
  message -- if the `TRUSTED_SIGNING_DLIB` environment variable is unset or
  points at a file that does not exist. That refusal is deliberate: it is
  what makes a silently-unsigned "successful" build impossible once signing
  is switched on.
- `trusted-signing-metadata.json` -- the Azure Trusted Signing endpoint,
  account, and certificate profile `sign.ps1` passes to `signtool /dmdf`.
  One certificate profile signs many products; this file is copied verbatim
  from the sibling `remo-code` repository's working, production signing
  setup, which already uses this same account and profile.
- `verify-fail-closed.ps1` -- the automated verify for this wiring. Drives
  `sign.ps1` through its three failure cases (no argument, dlib variable
  unset, dlib variable pointing at a missing file) and asserts every one
  exits non-zero. Run it with:

  ```
  pwsh -NoProfile -File app/src-tauri/signing/verify-fail-closed.ps1
  ```

## The committed Tauri config has no sign-command entry

`app/src-tauri/tauri.conf.json`, as committed to this repository, does not
set `bundle.windows.signCommand`. It never should. The release workflow
injects that entry into its own CI workspace copy of the file, for a signing
build only, immediately before `tauri build` runs -- so every local, dev,
and PR build stays exactly as it is today: unsigned, and never contacting
Azure. A guard test (`app/src-tauri/tests/signing_wiring.rs`) fails CI if a
sign-command entry is ever accidentally committed.

## Switching signing on: what an operator provisions

No code change is needed to switch signing on. An operator provisions, on
this GitHub repository:

- Repository secret `AZURE_CLIENT_ID`
- Repository secret `AZURE_CLIENT_SECRET`
- Repository secret `AZURE_TENANT_ID`
- Repository variable `ENABLE_MSI_SIGNING` set to the string `true`

These three secrets and one variable are the Azure Trusted Signing
service-principal credentials for the `titaniumlabs-signing` account /
`TitaniumLabsLLC` certificate profile. As of this writing they are NOT
provisioned for this repository -- the signed, publicly-released path is
blocked on that operational step, not on any code in this directory or the
release workflow. See `docs/signing.md` for the full operator procedure,
including the manual signature-verification command and the deliberate
fail-closed check to run once credentials exist.

## Certificate lifetime

Azure Trusted Signing certificates are short-lived by design (about three
days). This is not a defect and needs no renewal logic: the RFC-3161
timestamp countersignature `sign.ps1` requests (`/tr` + `/td SHA256`)
preserves the produced signature's validity indefinitely after the
certificate itself expires. Do not add expiry-check or renewal code here.

## Prohibitions

- Never self-sign, generate a per-app certificate, or provision a second
  certificate profile for this app. Azure Trusted Signing holds the key
  material; that is the entire point of the service.
- Never place a `.pfx` file or any private key material in this repository.
- Never echo, print, log, or interpolate the Azure client id, client secret,
  or tenant id anywhere in this directory's scripts or in any workflow step
  that consumes them -- not even while troubleshooting. `sign.ps1` prints
  only file PATHS for diagnostics.
- Never add a post-bundling `signtool` pass. Signing must happen DURING
  Tauri's bundling step, through `bundle.windows.signCommand`, because a
  post-bundle pass runs after an updater signature would be computed and
  silently breaks update verification. This app has no updater today, but
  the ordering is correct now so a future updater does not inherit a broken
  pipeline.
