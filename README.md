# golem

A small-fleet declarative orchestrator: write workloads and ingresses in Nickel, and per-node agents reconcile bare-metal Debian state (packages, files, systemd units, Podman quadlets) with refcounted ownership and surgical undo.