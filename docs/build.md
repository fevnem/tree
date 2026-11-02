# Building Tree on Linux

Tree is developed and cross-built on Linux; `cargo-xwin` supplies the MSVC SDK
and CRT, `llvm-rc` compiles the icon/manifest/version block.

## Prerequisites

```bash
sudo apt install -y clang lld llvm nsis pkg-config
rustup target add x86_64-pc-windows-msvc
cargo install --locked cargo-xwin
```

Rust 1.99 or newer. Nothing else: no Windows host, no Visual Studio, no WiX.

## Build

```bash
./scripts/build-windows.sh     # → dist/tree.exe  (+ size and sha256)
./scripts/package.sh           # → portable zip, NSIS installer, SHA256SUMS
python3 scripts/make-icon.py   # regenerates res/tree.ico (only if the art changes)
```

Release profile: `lto = "fat"`, `codegen-units = 1`, `panic = "abort"`,
`strip = true`, static CRT. The result is a single self-contained exe with no
VC++ redistributable and no runtime dependency.

## Tests

```bash
cargo test -p tree-core        # engine, runs on the build host against a live URL
```

Three integration tests hit the network on purpose — the point of the engine is
what it does to a real server:

* `eight_connections_match_a_plain_download` — 8 ranged connections vs one plain
  GET, compared by SHA-256;
* `pause_then_resume_keeps_every_byte` — pause mid-transfer, resume, hash must
  still match;
* `rejects_non_http_input_and_deduplicates` — nothing but http(s) is queued and
  the same URL is not queued twice while in flight.

Test URL: `https://proof.ovh.net/files/10Mb.dat` (10 MiB, honours ranges).

## Verifying the artifact

```bash
file dist/tree.exe                     # PE32+ executable (GUI) x86-64
llvm-readobj --coff-resources dist/tree.exe | grep '^  Type:'
#   ICON, GROUP_ICON, MANIFEST, VERSIONINFO
llvm-readobj --coff-imports dist/tree.exe | grep Name:
#   KERNEL32 user32 comctl32 shell32 dwmapi uxtheme gdi32 secur32
#   ws2_32 crypt32 bcryptprimitives ole32 oleaut32 advapi32 combase ntdll
```

No `VCRUNTIME140.dll`, no `libssl`, no WebView2: if an unexpected import shows
up, a dependency crept in.

## Known build noise

`lld-link: Cannot use debug info for 'libcmt.lib…' [LNK4099]` — the MSVC static
libraries ship without PDBs. Harmless, expected in every cross-build.
