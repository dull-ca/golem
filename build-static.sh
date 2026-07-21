#!/usr/bin/env bash
# build-static.sh — produce static x86_64-linux-musl binaries for golemd and golemctl.
#
# Requires:
#   * `zig` on PATH (any 0.13+)
#   * `rustup target add x86_64-unknown-linux-musl`
#
# Output:
#   target/x86_64-unknown-linux-musl/release/{golemd,golemctl}
#
# Both come out as fully static, ~3-5 MB after `strip` (already enabled in
# the release profile). Drop-in deployable on any glibc Debian/Ubuntu/RHEL/Alpine.

set -euo pipefail

cd "$(dirname "$0")"

cat > /tmp/zigcc <<'EOF'
#!/usr/bin/env bash
args=()
for a in "$@"; do
  case "$a" in
    --target=x86_64-unknown-linux-musl|--target=x86_64-unknown-linux-gnu) ;;
    *) args+=("$a") ;;
  esac
done
exec zig cc -target x86_64-linux-musl "${args[@]}"
EOF
chmod +x /tmp/zigcc

cat > /tmp/zigar <<'EOF'
#!/usr/bin/env bash
exec zig ar "$@"
EOF
chmod +x /tmp/zigar

RUST_LLD="$(dirname "$(dirname "$(command -v rustc)")")/lib/rustlib/x86_64-unknown-linux-gnu/bin/rust-lld"
if [ ! -x "$RUST_LLD" ]; then
  echo "rust-lld not found at $RUST_LLD" >&2
  exit 1
fi

export CC_x86_64_unknown_linux_musl=/tmp/zigcc
export AR_x86_64_unknown_linux_musl=/tmp/zigar
export CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER="$RUST_LLD"
export CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_RUSTFLAGS="-C linker-flavor=ld.lld -C link-self-contained=yes"

cargo build --release --target x86_64-unknown-linux-musl -p golemd -p golemctl

ls -lh target/x86_64-unknown-linux-musl/release/golemd \
       target/x86_64-unknown-linux-musl/release/golemctl
file     target/x86_64-unknown-linux-musl/release/golemd
