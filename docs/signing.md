# Windows code signing (Azure Trusted Signing)

This document is for an operator provisioning or verifying Windows
Authenticode signing for the JWL Manager Tauri app. It complements
`app/src-tauri/signing/README.md`, which documents the committed files
themselves; this document covers the end-to-end operational procedure.

## Provisioning

Signing -- and, as a direct consequence, public GitHub Release publishing --
is OFF today. Azure Trusted Signing service-principal credentials for this
repository are NOT YET PROVISIONED. The three repository secrets and the one
repository variable that switch it on are currently absent from this
repository:

- Repository secret `AZURE_CLIENT_ID`
- Repository secret `AZURE_CLIENT_SECRET`
- Repository secret `AZURE_TENANT_ID`
- Repository variable `ENABLE_MSI_SIGNING`, set to the exact string `true`

These identify a service principal with signing rights on the
`titaniumlabs-signing` Azure Trusted Signing account, scoped to the
`TitaniumLabsLLC` certificate profile (the same account and profile already
used in production by the sibling `remo-code` repository -- one certificate
profile signs many products; no per-app profile is provisioned here). The
service principal needs the "Trusted Signing Certificate Profile Signer"
role (or equivalent) assigned on that certificate profile.

No code change is needed to switch signing on. Provisioning the three
secrets and the one variable is sufficient: the next tag push matching
`app-v*.*.*` will build, sign during bundling, and publish a public GitHub
Release, exactly as `.github/workflows/release-app.yml` already implements
today, gated on these four values.

## Manual verification after credentials exist

This step CANNOT be performed in the current environment -- no Azure
credentials exist here, and none can be provisioned from it. It is the
single piece of PLAT-02 that remains manually verified, and it must be done
by a human operator after credentials are provisioned and before trusting
any subsequently "green" signed build.

After provisioning the four values above and pushing an `app-v*.*.*` tag,
download the produced `.msi` (or `.exe`) artifact from the resulting public
GitHub Release, then on a Windows machine run:

```
signtool verify /pa /v "JWL Manager_<version>_x64_en-US.msi"
```

A PASS looks like `signtool` reporting `Successfully verified` along with
the Authenticode signer identity and an RFC-3161 timestamp countersignature
entry. A FAILURE (`SignTool Error: No signature found` or a broken chain)
means either the enable variable was not actually `true` at build time, the
sign-command injection did not run, or the dlib install failed silently --
treat any failure as signing being effectively off, not as a false-positive
"mostly signed" state.

## Deliberate fail-closed check

Do this once, the first time credentials are provisioned, BEFORE trusting
that a green signing build means the artifact is actually signed. A green
build is only evidence of signing after this check has been done: a build
with a mis-resolved script path (RESEARCH Pitfall 1) can, in principle, look
identical to a genuinely signed build if the injected `signCommand` were
ever pointed at the wrong path and something else happened to sit there --
this check proves the hook is actually being invoked and actually fails
when it should.

1. Provision the three Azure secrets and `ENABLE_MSI_SIGNING=true` as above.
2. Temporarily break the dlib resolution -- for example, set
   `TRUSTED_SIGNING_DLIB` to a path that does not exist by editing the
   install step's output in a scratch branch, or otherwise force the
   Trusted Signing dlib install step to produce a bad path.
3. Push a tag matching `app-v*.*.*` (or trigger the workflow manually
   against that scratch state) and confirm the build FAILS at the
   `sign.ps1` invocation, not silently succeeds.
4. Revert the scratch change and confirm a normal signing build then
   succeeds and the resulting artifact passes `signtool verify /pa`.

If step 3 does not fail, STOP: the sign-command hook is not actually wired
to `sign.ps1`, and the build's earlier "success" was not evidence of
signing. Do not proceed to trust any signed build until step 3 genuinely
fails and step 4 genuinely passes.

## Certificate lifetime

Azure Trusted Signing certificates are short-lived by design (about three
days). This is expected, not a defect, and needs no renewal or expiry-check
logic: `sign.ps1` requests an RFC-3161 timestamp countersignature
(`/tr http://timestamp.acs.microsoft.com /td SHA256`) on every signature,
which preserves the produced signature's validity indefinitely after the
signing certificate itself expires. Do not add renewal, rotation, or
expiry-check logic anywhere in this pipeline -- doing so would be a defect,
not an improvement.

## Residual risks

- **The vendored `jwlCore` native library is not signed by this pipeline.**
  It is a prebuilt binary (`libs/jwlCore-amd64.dll` and platform siblings)
  that this repository does not build; `bundle.windows.signCommand` only
  signs what `tauri build` itself produces (the MSI/NSIS installer and its
  bundled `.exe`). Windows SmartScreen reputation applies primarily to the
  top-level installer a user directly runs, not to a resource DLL loaded at
  runtime via `libloading` -- so this is accepted as out of scope for this
  phase. If SmartScreen friction is ever observed specifically on that
  library, that is a re-scope, not evidence this pipeline is broken.
- **macOS notarization is out of scope.** It needs an Apple Developer
  credential, which does not exist in this environment. macOS users
  continue to need `xattr -cr` to bypass Gatekeeper, exactly as documented
  for the existing Python app.
- **There is no auto-updater today.** Because none exists, there is no
  updater `.sig` to protect yet -- but the signing hook is wired to run
  DURING Tauri's bundling step (via `bundle.windows.signCommand`), not as a
  post-build pass, specifically so that a future updater inherits a
  correctly-ordered pipeline instead of a broken one. A post-build signtool
  pass would run after an updater signature is computed and silently break
  update verification; this pipeline never does that, by construction.
