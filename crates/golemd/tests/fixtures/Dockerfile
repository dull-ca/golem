# Debian trixie with systemd-as-PID-1, suitable for integration tests
# of golemd's apt + systemd providers.
#
# Build:
#   docker build -t golem-smoke:trixie crates/golemd/tests/fixtures
#
# Run notes:
#   - Container must be started with --privileged or the equivalent
#     cgroup + tmpfs mounts; systemd needs cgroup management. Docker's
#     `--systemd=true` flag (or `--cgroupns=host` + tmpfs mounts) is the
#     usual way. testcontainers-rs's with_privileged(true) covers it.
#   - The agent binary is bind-mounted at runtime by the test harness;
#     it does NOT live in the image. This avoids rebuilding the image
#     on every code change.

FROM debian:trixie-slim

ENV DEBIAN_FRONTEND=noninteractive
ENV container=docker

# systemd + the bits the agent actually shells to.
# We install dbus, ca-certificates (apt over https), curl (test diagnostics),
# sqlite3 (journal inspection from inside the container), and procps (ps/kill).
RUN apt-get update && apt-get install -y --no-install-recommends \
        systemd systemd-sysv dbus ca-certificates \
        curl sqlite3 procps \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/*

# Standard systemd-in-container plumbing: drop the units that don't make
# sense inside a container and keep journald running in foreground.
RUN find /etc/systemd/system /lib/systemd/system -path '*.wants/*' \
        \( -name '*udev*' -o -name '*swap*' -o -name '*remount*' \) \
        -exec rm -f {} + \
    && systemctl set-default multi-user.target

VOLUME [ "/sys/fs/cgroup", "/run", "/run/lock", "/tmp" ]

STOPSIGNAL SIGRTMIN+3
ENTRYPOINT ["/sbin/init"]
