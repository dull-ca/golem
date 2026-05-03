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

# Tell cargo to use zig as the linker for the musl target.
export CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER=$(command -v zigcc 2>/dev/null || echo "")
if [ -z "$CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER" ]; then
  cat > /tmp/zigcc <<'EOF'
#!/usr/bin/env bash
exec zig cc -target x86_64-linux-musl "$@"
EOF
  chmod +x /tmp/zigcc
  export CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER=/tmp/zigcc
fi
export CC_x86_64_unknown_linux_musl=$CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER
export AR_x86_64_unknown_linux_musl=$(command -v zig)
# zig as ar shim:
cat > /tmp/zigar <<'EOF'
#!/usr/bin/env bash
exec zig ar "$@"
EOF
chmod +x /tmp/zigar
export AR_x86_64_unknown_linux_musl=/tmp/zigar

cargo build --release --target x86_64-unknown-linux-musl

ls -lh target/x86_64-unknown-linux-musl/release/golemd \
       target/x86_64-unknown-linux-musl/release/golemctl
file     target/x86_64-unknown-linux-musl/release/golemd
