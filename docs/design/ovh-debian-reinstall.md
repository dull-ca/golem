# Push-button Debian reinstall on an OVH bare-metal server

How to wipe and reinstall Debian onto the OVH dedicated server, repeatably,
from one command. Research date: **2026-07-30**, verified against the live OVH
API (`eu.api.ovh.com`), the current OVH docs, and the Terraform provider
source. Sources at the bottom; claims we could not verify are listed under
*Unverified*, not silently assumed.

## The one thing to know first: the API changed in 2025

Everything written before mid-2025 is wrong now. OVH removed the old
`POST /dedicated/server/{serviceName}/install/start` flow and the entire
personal-installation-template system (`/me/installationTemplate`) — the
routes return 404 today (June 17, 2025: templates frozen; October 7, 2025:
deleted). The replacement collapses the whole flow into **one call**:

```
POST /dedicated/server/{serviceName}/reinstall
```

with the OS name, customizations, and the full disk layout inline in the
body. Staleness heuristic for any tutorial: if it says `install/start`,
`template_name`, or `ovh_dedicated_server_install_task`, discard it.

## The model

- **OS**: an OVH catalogue template, by name. Current Debian identifiers
  (live): `debian12_64` (Bookworm, installable until 2028-07-04) and
  `debian13_64` (Trixie, until 2030-07-02). Discover what *this* server
  accepts via `GET /dedicated/server/{serviceName}/install/compatibleTemplates`.
- **Partitioning**: inline in the call (`storage[].partitioning.layout`), one
  entry per partition: `fileSystem` (ext4/xfs/btrfs/zfs/swap/…), `mountPoint`,
  `size` in MiB (`0` = fill the disk, at most one such), `raidLevel`
  (software RAID; **defaults to 1** if unstated), optional LVM (`extras.lv.name`)
  and zpool (`extras.zp.name`) naming. Hardware RAID, if the box has a
  controller, via `storage[].hardwareRaid`. Only one disk group is supported
  per install; discover groups via
  `GET /dedicated/server/{serviceName}/specifications/hardware`.
- **Customizations** (all optional for Debian, live-checked): `hostname`,
  `sshKey` (one public key), `postInstallationScript` (base64; any shebang),
  `configDriveMetadata`, `enableLacpBonding`.
- **Result**: a task id. Poll it, plus a step-by-step progress endpoint.

## One-time setup: credentials

Two supported headless mechanisms; the service account is the better fit.

**OAuth2 service account (preferred)** — a clientId/clientSecret pair with no
expiry, bound to an IAM policy. Create via `POST /me/api/oauth2/client`
(`flow: CLIENT_CREDENTIALS`); attach its URN to a policy granting, minimally:
`dedicatedServer:apiovh:reinstall` plus the reads
(`…:task/get`, `…:install/status/get`, `…:install/compatibleTemplates/get`).

**Classic three-key** — visit `https://auth.eu.ovhcloud.com/api/createToken`,
scope the rights to `POST /dedicated/server/<serviceName>/reinstall` and the
matching `GET`s, and you get AK/AS/CK in one shot, no interactive validation.

Either way the credentials land in `~/.ovh.conf` or environment variables
(`OVH_ENDPOINT=ovh-eu`, `OVH_APPLICATION_KEY`, `OVH_APPLICATION_SECRET`,
`OVH_CONSUMER_KEY`), read by both the official CLI and python-ovh.

## The box spec: one JSON file, parameterized per box

Everything box-specific — the disks, the layout, the hostname — lives in one
committed file. Example for a two-disk box, software RAID 1 across both:

```json
{
  "operatingSystem": "debian13_64",
  "customizations": {
    "hostname": "golem-ci-01",
    "sshKey": "ssh-ed25519 AAAA... lakin@steel",
    "postInstallationScript": "<base64, see The golem seam below>"
  },
  "storage": [
    {
      "partitioning": {
        "disks": 2,
        "layout": [
          { "fileSystem": "ext4", "mountPoint": "/boot", "size": 1024, "raidLevel": 1 },
          { "fileSystem": "swap", "mountPoint": "swap",  "size": 4096 },
          { "fileSystem": "ext4", "mountPoint": "/",     "size": 0,    "raidLevel": 1 }
        ]
      }
    }
  ]
}
```

Debian-specific layout rules, verified from OVH's partitioning guide:

- `/boot` (or `/` if no separate `/boot`) **cannot be XFS** on Debian-family
  OSes.
- ZFS root needs ≥ 8 GiB RAM (Debian compiles the module at install).
- OVH appends its own **cloud-init config-drive partition** at the end of the
  disk; on non-GPT servers your partitions must end before ~2 TiB.
- OVH **silently adjusts** some requests (RAID level reduction, size
  trimming, LV grouping) — after the first install, diff the real layout
  (`lsblk`, `df`) against the spec and adjust the spec to match reality.
- Unstated `raidLevel` means RAID 1, mirroring across all listed disks.

## The button

Official CLI (`github.com/ovh/ovhcloud-cli`; bare-metal coverage is
first-class, and `--wait` blocks until done then fetches the new machine's
credentials):

```nushell
ovhcloud baremetal reinstall <serviceName> --from-file boxes/golem-ci-01.json --wait
```

That is the entire wipe-and-redeploy. Note the CLI's individual flags don't
cover `storage` — custom partitioning requires `--from-file` (or stdin),
which is what we want anyway: the file is the source of truth.

Equivalent python (`pip: ovh`, official), for when this becomes a golem-driven
step rather than a hand-pressed button:

```python
import ovh, time

client = ovh.Client()                      # reads OVH_* env / ~/.ovh.conf
server = "nsXXXXXXX.ip-XX-XX-XX.eu"
spec = json.load(open("boxes/golem-ci-01.json"))

task = client.post(f"/dedicated/server/{server}/reinstall", **spec)

while True:
    t = client.get(f"/dedicated/server/{server}/task/{task['taskId']}")
    if t["status"] in ("done", "cancelled", "customerError", "ovhError"):
        break
    for step in client.get(f"/dedicated/server/{server}/install/status")["progress"]:
        print(step["status"], step["comment"])
    time.sleep(30)
```

An OpenTofu spelling exists (`ovh_dedicated_server_reinstall_task`, every
field `ForceNew`, so `tofu apply -replace=…` is the button), but it adds a
state file and a provider for what is genuinely one API call — the CLI/python
forms fit this repo better. If we ever adopt it: its `Read` is deliberately a
no-op (OVH purges old tasks), so an unchanged config never re-triggers.

## Watching it

- `GET /dedicated/server/{serviceName}/task/{taskId}` — coarse:
  `init → todo → doing → done` (or `customerError` / `ovhError` — the split
  is meaningful: *customer* = the spec asked for something unbuildable,
  *ovh* = their fault, retry/ticket).
- `GET /dedicated/server/{serviceName}/install/status` — per-step progress
  with comments and error text. **Poll this, not just the task**: some
  partitioning errors are only detected mid-install, after the API accepted
  the POST.
- The Terraform provider's own polling budget is instructive: 10 s initial
  delay, ≥ 3 s interval, 60-minute timeout, and it retries 404/500 for up to
  5 minutes because "the Dedicated Server API often returns 500/404 errors."
  Build the same tolerance into any script.
- Typical wall-clock duration is not documented anywhere; measure the first
  run and encode that as the timeout baseline.

First boot facts (from OVH's getting-started guide): root SSH is disabled by
default on template installs; a distro-named default user is created and
initial credentials are emailed — but with `sshKey` set, key-based login is
provisioned and no password flow is needed. Verify which user the key lands
on (root vs `debian`) on the first install and record it here.

## The golem seam

`postInstallationScript` is a base64 imperative shell blob — exactly the kind
of thing golem exists to replace. Keep it to a bootstrap that hands control
to the reconciler and nothing more:

```bash
#!/bin/bash
set -euo pipefail
curl -fsSL <artifact-url>/golemd -o /usr/local/bin/golemd
chmod +x /usr/local/bin/golemd
# minimal systemd unit + enable, pointing at the manifest source
```

Everything after that — packages, services, files, the CI loop — is the
host's scroll in the fleet manifest. The install spec owns "blank Debian with
disks arranged"; golem owns everything else. (Where golemd's own binary is
served from is the open release-publishing question, ADR 0035 §5.)

## Alternatives considered and set aside

- **BYOI / BYOLinux** (`byoi_64` / `byolinux_64`, same reinstall endpoint):
  boot a self-built qcow2/raw image (must fit in RAM − 3 GiB, must contain
  cloud-init; BYOLinux additionally: single partition + a
  `make_image_bootable.sh`). The escape hatch if we ever want pre-baked
  golden images or outlive a template's `endOfInstall` — overkill for stock
  Debian.
- **Rescue mode + debootstrap** (incl. `grml-debootstrap`'s unattended mode)
  and **kexec-into-debian-installer** (`sergelogvinov/ansible-role-debian-boot`,
  preseed-driven — the Debian analogue of the nixos-anywhere pattern):
  workable, fully automatable via API-triggered rescue boots
  (`rescueSshKey` avoids the credentials email entirely), but hand-rolled
  where the reinstall API is supported, and with all the classic GRUB /
  network-config / interface-naming footguns. Worth revisiting only if the
  catalogue install proves too rigid.
- **Custom iPXE** (`bootScript` on the server, or `/me/ipxeScript`):
  supported, but no install-progress API and the most moving parts.
- OVH has no equivalent of Hetzner's `installimage`; the reinstall API *is*
  their supported automation story.

## Unverified — test on the first throwaway reinstall

1. Whether `configDriveUserData` (full cloud-init user-data) is honored on
   stock `debian13_64` — the model accepts it globally, but the Debian
   templates advertise only `configDriveMetadata`, and every worked example
   is BYOLinux. If it works, it could replace `postInstallationScript`
   entirely (declarative > imperative). If not, `postInstallationScript` is
   the verified hook.
2. Whether `sshKey` accepts more than one key (type says singular).
3. Which user the SSH key lands on (root is disabled by default).
4. Actual install duration for this box (undocumented; needed for timeouts).

## Sources

Live API schema: `https://eu.api.ovh.com/1.0/dedicated/server.json`,
`…/1.0/dedicated/installationTemplate` (+ per-template). Guides (docs.ovhcloud.com
/ github.com/ovh/docs): *Install an OS via the OVHcloud API* (2025-06-06),
*Configuring storage/partitioning via API* (2026-02-18), *End of life for
personal installation templates*, *Bring Your Own Image* / *Bring Your Own
Linux* (2026-05-11), *Rescue mode*, *iPXE scripts*, *Getting started with a
dedicated server* (2025-04-29), *First steps with the API* (2025-05-13),
*Service accounts*. Tooling: `github.com/ovh/ovhcloud-cli` (reinstall command
source), `github.com/ovh/python-ovh` (v1.2.0),
`github.com/ovh/terraform-provider-ovh` (v2.18.0; CHANGELOG v2.0.0 breaking
change 2025-03-04; `resource_dedicated_server_reinstall_task.go`,
`dedicated_server_task.go`). Community: raghavsood.com 2024-06-21 (OVH kexec
hang + `/dev/sda` constraint), `nix-community/nixos-anywhere`,
`sergelogvinov/ansible-role-debian-boot`, `grml/grml-debootstrap`.
