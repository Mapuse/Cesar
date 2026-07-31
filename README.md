##

```
 ██████╗███████╗███████╗ █████╗ ██████╗
██╔════╝██╔════╝██╔════╝██╔══██╗██╔══██╗
██║     █████╗  ███████╗███████║██████╔╝
██║     ██╔══╝  ╚════██║██╔══██║██╔══██╗
╚██████╗███████╗███████║██║  ██║██║  ██║
 ╚═════╝╚══════╝╚══════╝╚═╝  ╚═╝╚═╝  ╚═╝
```

##

`▐▀` `-` `▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▌`

The Init System of **`[Cudane]`**, a Good Replace of **`[Systemd]`** Written in **`[Rust]`** That Completely **`[Silent]`**, So You Won't Be Scared Again of Terrifying Streaming Output, Cesar runs as the first process during boot, manages all system services through an **`[Async]`** dependency graph, and provides a **`[CLI]`** for administration after boot completes.

- **`[Version]`**: **`[0.0.7]`**

`▐▄` `-` `▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▌`

<details>
<summary>Contents</summary>

- **`[[Overview]]`**(#overview)
- **`[[Architecture]]`**(#architecture)
- **`[[Installation]]`**(#installation)
- **`[[Binaries]]`**(#binaries)
- **`[[Services]]`**(#services)
- **`[[CLI]]`**(#cli)
  - **`[[Flags]]`**(#flags)
  - **`[[Conventions]]`**(#conventions)
  - **`[[service]]`**(#service)
  - **`[[system]]`**(#system)
  - **`[[config]]`**(#config)
  - **`[[log]]`**(#log)
  - **`[[socket](#socket)
  - **`[[daemon]]`**(#daemon)
  - **`[[snapshot]]`**(#snapshot)
  - **`[[security]]`**(#security)
  - **`[[query]]`**(#query)
  - **`[[debug]]`**(#debug)
  - **`[[self]]`**(#self)
- **`[[CLI]]`**(##cli)
- **`[[Boot]]`**(#boot)
- **`[[Resolution]]`**(#resolution)
- **`[[State]]`**(#state)
- **`[[Logging]]`**(#logging)
- **`[[Sockets]]`**(#sockets)
- **`[[Snapshots]]`**(#snapshots)
- **`[[Models]]`**(#models)
- **`[[Filesystem]]`**(#filesystem)
- **`[[Building]]`**(#building)
- **`[[Python]]`**(#python)
- **`[[Configuration]]`**(#configuration)
- **`[[Structure]]`**(#structure)
- **`[[Dependencies]]`**(#dependencies)

---

</details>

<details>
<summary>Overview</summary>

- Flat `.ini` Services files
- Async parallel boot via Tokio
- DAG-based Resolution with cycle detection
- Auto-restart watchdog (`Restart=always` / `Restart=on-failure`)
- Sockets (Unix stream/datagram, TCP)
- Markdown structure logging at `/var/log/cesar.md`
- Plymouth splash integration (silent by default)
- Process group isolation with signal forwarding
- Environment variables and working directory per service
- Snapshot and rollback system
- Security auditing (binary permissions, capabilities, seccomp, network listeners)

---

</details>

<details>
<summary>Architecture</summary>

```
┌──────────────────────────────────────────────────┐
│                      PID 1                       │
│   ┌──────────┐  ┌──────────┐  ┌──────────────┐   │
│   │  Signal  │  │  DAG     │  │   Process    │   │
│   │  Handler │  │  Engine  │  │   Manager    │   │
│   └────┬─────┘  └────┬─────┘  └──────┬───────┘   │
│        │             │               │           │
│   ┌────┴─────┐  ┌────┴─────┐  ┌──────┴───────┐   │
│   │  Logger  │  │  Config  │  │   Socket     │   │
│   │ (MD log) │  │  Parser  │  │  Activator   │   │
│   └──────────┘  └──────────┘  └──────────────┘   │
└──────────────────────────────────────────────────┘
         │                             │
    ┌────┴─────┐                  ┌────┴─────┐
    │ Plymouth │                  │  /proc   │
    │ (splash) │                  │  /sys    │
    └──────────┘                  │  /dev    │
                                  └──────────┘
```

| Component | File | Purpose |
|-----------|------|---------|
| Signal Handler | `main.rs` | SIGCHLD (zombie reaping), SIGTERM/SIGINT (graceful shutdown), SIGHUP (config reload) |
| DAG Engine | `dag.rs` | Topological sort, cycle detection, parallel boot ordering |
| Process Manager | `process.rs` | `fork`/`exec` with process groups and session isolation |
| Config Parser | `config.rs` | Flat `.ini` parser for `/system/lib/cesar/services/` and `/etc/cesar/services/` |
| Logger | `logger.rs` | Markdown-formatted log at `/var/log/cesar.md` |
| Socket Activator | `socket.rs` | Unix/TCP socket creation and activation |
| Visual Diagnostics | `visual.rs` | Error trees with `[CTX]` and `[FIX]` annotations |
| CLI | `cli.rs` | 2,162-line Clap-based CLI with 11 command groups |
| Commands | `commands.rs` | 1,993 lines — every handler does real work |

---

</details>

<details>
<summary>Binaries</summary>

| Binary | Path | Purpose |
|--------|------|---------|
| `csr` | `/system/bin/csr` | Main binary — PID 1 during boot, CLI tool after boot |
| `csl` | `/system/bin/csl` | Log viewer — reads and formats `/var/log/cesar.md` |

---

</details>

<details>
<summary>Services</summary>

Services are defined as flat `.ini` files in `/system/lib/cesar/services/` or `/etc/cesar/services/`.

### Format

```ini
Name = my-service
Exec = /system/bin/my-service --flag
Requires = dep1, dep2
Restart = always
Socket = unix:/run/my-service.sock
Description = My awesome service
Environment = HOME=/var/lib/my-service level=debug
WorkingDirectory = /var/lib/my-service
```

### Fields

| Field | Required | Default | Description |
|-------|----------|---------|-------------|
| `Name` | Yes | — | Unique service identifier |
| `Exec` | Yes | — | Command to execute (absolute path) |
| `Requires` | No | none | Comma-separated list of dependency service names |
| `Restart` | No | `never` | `never`, `on-failure`, or `always` |
| `Socket` | No | none | Socket spec: `unix:/path/to.sock` or `tcp:host:port` |
| `Description` | No | none | Human-readable description |
| `Environment` | No | none | Space-separated `KEY=VALUE` pairs passed to the process |
| `WorkingDirectory` | No | none | Working directory set before `exec()` |

### Example: udevd

```ini
Name = udevd
Exec = /system/bin/udevd
Description = Device manager daemon
Restart = always
```

### Example: greetd with dependencies

```ini
Name = greetd
Exec = /system/bin/greetd
Requires = seatd, hw-detect, iwd, dhcpcd, wireplumber
Description = Login greeter daemon
Restart = always
```

### Example: service with environment and working directory

```ini
Name = my-daemon
Exec = /system/bin/my-daemon
Description = Custom daemon
Restart = always
WorkingDirectory = /var/lib/my-daemon
Environment = HOME=/var/lib/my-daemon level=info DB_PATH=/var/lib/my-daemon/data.db
```

### Config search order

1. `/system/lib/cesar/services/*.ini` (system, loaded first)
2. `/etc/cesar/services/*.ini` (user, higher priority — overrides system configs with the same `Name`)

---

</details>

<details>
<summary>CLI</summary>

### Flags

```
csr [OPTIONS] [COMMAND]

Options:
  -h, --help      Print help
  -V, --version   Print version
```

### Conventions

- All subcommands support `-` and `--` flags interchangeably
- Mixed flagging is supported: `csr service -n greetd -f` is the same as `csr service --name greetd --force`
- Every command group has a short alias: `csr svc` = `csr service`, `csr sys` = `csr system`, etc.
- Every subcommand has short aliases where applicable
- All output commands support `--json` for machine-readable output
- Boolean flags default to `false` when omitted
- Timeout flags default to `30` seconds (services) or `60` seconds (system operations) when omitted

---

### service

Service management and lifecycle control. Alias: `svc`

```
csr service <COMMAND>

Commands:
  start     Start a service                    (alias: up)
  stop      Stop a service                     (alias: dn)
  restart   Restart a service                  (alias: rs)
  reload    Reload Services       (alias: rl)
  kill      Send signal to a service
  enable    Enable a service for boot
  disable   Disable a service
  status    Show service status
  list      List all services                  (alias: ls)
  inspect   Inspect service details
  log       View service logs
  cat       Show service config file
  edit      Edit service config
  diff      Diff running vs on-disk config
  validate  Validate Services
  create    Create a new service config
  rm        Remove a service
  monitor   Monitor service health
  watch     Watch service events in real-time
  tree      Show dependency tree               (alias: dep)
```

#### `service start`

Start a service. Requires `--name`.

| Flag | Short | Long | Type | Default | Description |
|------|-------|------|------|---------|-------------|
| name | `-n` | `--name` | `String` | — | Service name (required) |
| wait | `-w` | `--wait` | `bool` | `false` | Wait for service to reach target state |
| timeout | `-t` | `--timeout` | `u64` | `30` | Timeout in seconds |

```sh
csr service start udevd
csr service start -n greetd
csr service start --name greetd --wait
csr service start -n greetd -w -t 60
csr svc up -n udevd
```

#### `service stop`

Stop a service. Requires `--name`.

| Flag | Short | Long | Type | Default | Description |
|------|-------|------|------|---------|-------------|
| name | `-n` | `--name` | `String` | — | Service name (required) |
| force | `-f` | `--force` | `bool` | `false` | Force operation even if transitional state |
| timeout | `-t` | `--timeout` | `u64` | `30` | Timeout in seconds |

```sh
csr service stop greetd
csr service stop -n greetd --force
csr service stop -n greetd -f -t 10
csr svc dn -n greetd -f
```

#### `service restart`

Restart a service. Requires `--name`.

| Flag | Short | Long | Type | Default | Description |
|------|-------|------|------|---------|-------------|
| name | `-n` | `--name` | `String` | — | Service name (required) |
| force | `-f` | `--force` | `bool` | `false` | Force operation |
| timeout | `-t` | `--timeout` | `u64` | `30` | Timeout in seconds |

```sh
csr service restart greetd
csr service restart -n greetd -f -t 10
csr svc rs -n greetd
```

#### `service reload`

Reload Services. Requires `--name`.

| Flag | Short | Long | Type | Default | Description |
|------|-------|------|------|---------|-------------|
| name | `-n` | `--name` | `String` | — | Service name (required) |
| signal | `-s` | `--signal` | `Option<String>` | SIGHUP | Signal to send |

```sh
csr service reload pipewire
csr service reload -n pipewire -s SIGHUP
csr svc rl -n pipewire
```

#### `service kill`

Send signal to a service. Requires `--name`.

| Flag | Short | Long | Type | Default | Description |
|------|-------|------|------|---------|-------------|
| name | `-n` | `--name` | `String` | — | Service name (required) |
| signal | `-s` | `--signal` | `Option<String>` | SIGTERM | Signal to send |

```sh
csr service kill -n snapd -s KILL
csr service kill --name snapd --signal SIGTERM
```

#### `service enable`

Enable a service for boot. Requires `--name`.

| Flag | Short | Long | Type | Default | Description |
|------|-------|------|------|---------|-------------|
| name | `-n` | `--name` | `String` | — | Service name (required) |
| boot | `-b` | `--boot` | `bool` | `false` | Enable for boot only |
| now | `-W` | `--now` | `bool` | `false` | Enable and start immediately |

```sh
csr service enable greetd
csr service enable -n greetd --now
csr service enable -n greetd -b
```

#### `service disable`

Disable a service. Requires `--name`.

| Flag | Short | Long | Type | Default | Description |
|------|-------|------|------|---------|-------------|
| name | `-n` | `--name` | `String` | — | Service name (required) |
| now | `-W` | `--now` | `bool` | `false` | Disable and stop immediately |

```sh
csr service disable acpid
csr service disable -n acpid --now
```

#### `service status`

Show service status. Name is optional (shows all if omitted).

| Flag | Short | Long | Type | Default | Description |
|------|-------|------|------|---------|-------------|
| name | `-n` | `--name` | `Option<String>` | — | Service name (omit for all) |
| json | `-j` | `--json` | `bool` | `false` | Output as JSON |
| quiet | `-q` | `--quiet` | `bool` | `false` | Quiet output |

```sh
csr service status
csr service status -n greetd
csr service status --json
csr service status -n greetd -j -q
```

#### `service list`

List all services.

| Flag | Short | Long | Type | Default | Description |
|------|-------|------|------|---------|-------------|
| all | `-a` | `--all` | `bool` | `false` | Show all including stopped |
| failed | `-f` | `--failed` | `bool` | `false` | Show only failed |
| running | `-r` | `--running` | `bool` | `false` | Show only running |
| sort | `-s` | `--sort` | `Option<String>` | — | Sort by field |
| json | `-j` | `--json` | `bool` | `false` | Output as JSON |
| minimal | `-m` | `--minimal` | `bool` | `false` | Minimal output |

```sh
csr service list
csr service list -a
csr service list -r
csr service list -f
csr service list -s name -j
csr service list --all --json --sort name
csr svc ls -a -j
```

#### `service inspect`

Inspect service details. Requires `--name`.

| Flag | Short | Long | Type | Default | Description |
|------|-------|------|------|---------|-------------|
| name | `-n` | `--name` | `String` | — | Service name (required) |
| raw | `-r` | `--raw` | `bool` | `false` | Show raw config |
| deps | `-d` | `--deps` | `bool` | `false` | Show dependencies |
| pid | `-p` | `--pid` | `bool` | `false` | Show PID info |
| json | `-j` | `--json` | `bool` | `false` | Output as JSON |

```sh
csr service inspect -n greetd
csr service inspect -n greetd -d -p
csr service inspect --name greetd --deps --pid --json
```

#### `service log`

View service logs.

| Flag | Short | Long | Type | Default | Description |
|------|-------|------|------|---------|-------------|
| name | `-n` | `--name` | `Option<String>` | — | Service name |
| follow | `-f` | `--follow` | `bool` | `false` | Follow log in real-time |
| lines | `-l` | `--lines` | `usize` | `50` | Number of lines |
| level | `-L` | `--level` | `Option<String>` | — | Filter by level |
| grep | `-g` | `--grep` | `Option<String>` | — | Grep pattern |
| since | `-S` | `--since` | `Option<String>` | — | Show since timestamp |

```sh
csr service log -n greetd
csr service log -n greetd -f
csr service log -n greetd -l 100 -L CRITICAL
csr service log -n greetd -g "not found"
csr service log --name greetd --follow --lines 200
```

#### `service cat`

Show service config file. Requires `--name`.

```sh
csr service cat -n greetd
csr service cat --name udevd
```

#### `service edit`

Edit service config. Requires `--name`.

| Flag | Short | Long | Type | Default | Description |
|------|-------|------|------|---------|-------------|
| name | `-n` | `--name` | `String` | — | Service name (required) |
| editor | `-e` | `--editor` | `Option<String>` | — | Editor to use |

```sh
csr service edit -n greetd
csr service edit -n greetd -e vim
```

#### `service diff`

Diff running vs on-disk config. Requires `--name`.

| Flag | Short | Long | Type | Default | Description |
|------|-------|------|------|---------|-------------|
| name | `-n` | `--name` | `String` | — | Service name (required) |
| running | `-r` | `--running` | `bool` | `false` | Compare with running state |

```sh
csr service diff -n greetd
csr service diff --name greetd --running
```

#### `service validate`

Validate Services.

| Flag | Short | Long | Type | Default | Description |
|------|-------|------|------|---------|-------------|
| name | `-n` | `--name` | `Option<String>` | — | Service name (omit for all) |
| all | `-a` | `--all` | `bool` | `false` | Validate all services |
| strict | `-s` | `--strict` | `bool` | `false` | Strict validation |

```sh
csr service validate -a
csr service validate -n greetd -s
csr service validate --all --strict
```

#### `service create`

Create a new service config. Requires `--name` and `--exec`.

| Flag | Short | Long | Type | Default | Description |
|------|-------|------|------|---------|-------------|
| name | `-n` | `--name` | `String` | — | Service name (required) |
| exec | `-e` | `--exec` | `String` | — | Exec path (required) |
| requires | `-r` | `--requires` | `Option<String>` | — | Required services (comma-separated) |
| restart | `-R` | `--restart` | `Option<String>` | — | Restart policy |
| socket | `-s` | `--socket` | `Option<String>` | — | Sockets spec |
| description | `-d` | `--description` | `Option<String>` | — | Description |
| environment | `-E` | `--environment` | `Option<String>` | — | Environment variables (space-separated KEY=VALUE) |
| working-directory | `-w` | `--working-directory` | `Option<String>` | — | Working directory |

```sh
csr service create -n my-svc -e /usr/bin/my-svc
csr service create --name my-svc --exec /usr/bin/my-svc --requires udevd --restart always
csr service create -n my-svc -e /usr/bin/my-svc -r udevd,seatd -R always -s unix:/run/my-svc.sock -d "My service"
csr service create -n my-svc -e /usr/bin/my-svc -E "HOME=/var/lib/my-svc level=debug" -w /var/lib/my-svc
```

#### `service rm`

Remove a service. Requires `--name`.

| Flag | Short | Long | Type | Default | Description |
|------|-------|------|------|---------|-------------|
| name | `-n` | `--name` | `String` | — | Service name (required) |
| force | `-f` | `--force` | `bool` | `false` | Force removal |
| purge | `-p` | `--purge` | `bool` | `false` | Remove config files too |

```sh
csr service rm -n my-svc
csr service rm --name my-svc --force --purge
```

#### `service monitor`

Monitor service health. Requires `--name`.

| Flag | Short | Long | Type | Default | Description |
|------|-------|------|------|---------|-------------|
| name | `-n` | `--name` | `String` | — | Service name (required) |
| interval | `-i` | `--interval` | `u64` | `1000` | Poll interval in ms |
| threshold | `-t` | `--threshold` | `u32` | `3` | Failure threshold before alert |

```sh
csr service monitor -n greetd
csr service monitor -n greetd -i 500 -t 5
csr service monitor --name greetd --interval 2000 --threshold 10
```

#### `service watch`

Watch service events in real-time.

| Flag | Short | Long | Type | Default | Description |
|------|-------|------|------|---------|-------------|
| name | `-n` | `--name` | `Option<String>` | — | Service name |
| events | `-e` | `--events` | `Option<String>` | — | Event types to watch |

```sh
csr service watch
csr service watch -n greetd
csr service watch -e start,stop,fail
```

#### `service tree`

Show dependency tree.

| Flag | Short | Long | Type | Default | Description |
|------|-------|------|------|---------|-------------|
| all | `-a` | `--all` | `bool` | `false` | Show all services |
| flat | `-f` | `--flat` | `bool` | `false` | Flat list output |
| graph | `-g` | `--graph` | `bool` | `false` | ASCII graph output |

```sh
csr service tree
csr service tree -a
csr service tree -g
csr service tree --all --graph
csr svc dep -a -g
```

---

### system

System-wide operations and control. Alias: `sys`

```
csr system <COMMAND>

Commands:
  boot       Boot the system with splash
  shutdown   Graceful shutdown
  reboot     Reboot the system
  poweroff   Power off the system
  emergency  Emergency mode
  suspend    Suspend to RAM
  resume     Resume from suspend
  freeze     Freeze all processes (cgroup freezer)
  thaw       Thaw frozen processes
  mount      Mount filesystem
  umount     Unmount filesystem
  sync       Sync filesystems
  hostname   Get/set hostname
  uptime     Show system uptime
  kernel     Kernel information
  env        Environment variable management
  resource   Resource monitoring
  cgroup     Cgroup management
  device     Device management
```

#### `system boot`

| Flag | Short | Long | Type | Default | Description |
|------|-------|------|------|---------|-------------|
| splash | `-s` | `--splash` | `bool` | `false` | Show splash screen |
| verbose | `-v` | `--verbose` | `bool` | `false` | Verbose output |
| single | `-S` | `--single` | `bool` | `false` | Single-user mode |
| emergency | `-e` | `--emergency` | `bool` | `false` | Emergency boot |

```sh
csr system boot
csr system boot -s -v
csr system boot --single
csr system boot --emergency
```

#### `system shutdown`

| Flag | Short | Long | Type | Default | Description |
|------|-------|------|------|---------|-------------|
| force | `-f` | `--force` | `bool` | `false` | Force shutdown |
| timeout | `-t` | `--timeout` | `u64` | `60` | Timeout in seconds |
| reboot | `-r` | `--reboot` | `bool` | `false` | Reboot after shutdown |

```sh
csr system shutdown
csr system shutdown -f
csr system shutdown -t 30
csr system shutdown --force --timeout 120
```

#### `system reboot`

| Flag | Short | Long | Type | Default | Description |
|------|-------|------|------|---------|-------------|
| force | `-f` | `--force` | `bool` | `false` | Force reboot |
| timeout | `-t` | `--timeout` | `u64` | `60` | Timeout in seconds |
| mode | `-m` | `--mode` | `Option<String>` | — | Reboot mode (warm/cold) |

```sh
csr system reboot
csr system reboot -f -t 30
csr system reboot --mode cold
```

#### `system poweroff`

| Flag | Short | Long | Type | Default | Description |
|------|-------|------|------|---------|-------------|
| force | `-f` | `--force` | `bool` | `false` | Force poweroff |
| timeout | `-t` | `--timeout` | `u64` | `60` | Timeout in seconds |

```sh
csr system poweroff
csr system poweroff -f -t 10
```

#### `system emergency`

| Flag | Short | Long | Type | Default | Description |
|------|-------|------|------|---------|-------------|
| reason | `-r` | `--reason` | `Option<String>` | — | Reason for emergency |

```sh
csr system emergency
csr system emergency -r "disk failure"
```

#### `system suspend`

| Flag | Short | Long | Type | Default | Description |
|------|-------|------|------|---------|-------------|
| hibernate | `-H` | `--hibernate` | `bool` | `false` | Hibernate instead of suspend |
| hybrid | `-y` | `--hybrid` | `bool` | `false` | Hybrid suspend |

```sh
csr system suspend
csr system suspend -H
csr system suspend --hybrid
```

#### `system resume`

No flags.

```sh
csr system resume
```

#### `system freeze`

| Flag | Short | Long | Type | Default | Description |
|------|-------|------|------|---------|-------------|
| timeout | `-t` | `--timeout` | `Option<u64>` | — | Timeout in seconds |

```sh
csr system freeze
csr system freeze -t 60
```

#### `system thaw`

No flags.

```sh
csr system thaw
```

#### `system mount`

| Flag | Short | Long | Type | Default | Description |
|------|-------|------|------|---------|-------------|
| source | `-s` | `--source` | `Option<String>` | — | Source device/path |
| target | `-t` | `--target` | `Option<String>` | — | Target mount point |
| type | `-T` | `--type` | `Option<String>` | — | Filesystem type |
| options | `-o` | `--options` | `Option<String>` | — | Mount options |
| recursive | `-r` | `--recursive` | `bool` | `false` | Recursive mount |
| remount | `-R` | `--remount` | `bool` | `false` | Remount |

```sh
csr system mount -s /dev/sda1 -t /mnt/data -T ext4
csr system mount --source /dev/sda1 --target /mnt/data --type ext4 --options rw,noatime
csr system mount -s /dev/sda1 -t /mnt/data -r
csr system mount -s /dev/sda1 -t /mnt/data -R
```

#### `system umount`

| Flag | Short | Long | Type | Default | Description |
|------|-------|------|------|---------|-------------|
| target | `-t` | `--target` | `Option<String>` | — | Target mount point |
| recursive | `-r` | `--recursive` | `bool` | `false` | Recursive unmount |
| lazy | `-l` | `--lazy` | `bool` | `false` | Lazy unmount |
| force | `-f` | `--force` | `bool` | `false` | Force unmount |
| detach | `-d` | `--detach` | `bool` | `false` | Detach loopback |

```sh
csr system umount -t /mnt/data
csr system umount -t /mnt/data -r
csr system umount -t /mnt/data -l -f
```

#### `system sync`

| Flag | Short | Long | Type | Default | Description |
|------|-------|------|------|---------|-------------|
| file-systems | `-f` | `--file-systems` | `Option<String>` | — | Sync specific filesystems |
| data | `-d` | `--data` | `bool` | `false` | Sync data only (no metadata) |

```sh
csr system sync
csr system sync -f /home,/var
csr system sync --data
```

#### `system hostname`

| Flag | Short | Long | Type | Default | Description |
|------|-------|------|------|---------|-------------|
| set | `-s` | `--set` | `Option<String>` | — | Set hostname |
| short | `-S` | `--short` | `bool` | `false` | Show short hostname |
| long | `-l` | `--long` | `bool` | `false` | Show long hostname |
| static | `-t` | `--static` | `bool` | `false` | Show static hostname |
| json | `-j` | `--json` | `bool` | `false` | Output as JSON |

```sh
csr system hostname
csr system hostname -s my-cudane
csr system hostname -S
csr system hostname -l
csr system hostname -j
csr system hostname --set my-cudane --json
```

#### `system uptime`

| Flag | Short | Long | Type | Default | Description |
|------|-------|------|------|---------|-------------|
| since | `-s` | `--since` | `bool` | `false` | Show since boot time |
| seconds | `-S` | `--seconds` | `bool` | `false` | Show in seconds |
| json | `-j` | `--json` | `bool` | `false` | Output as JSON |

```sh
csr system uptime
csr system uptime -s
csr system uptime -S
csr system uptime -j
```

#### `system kernel`

Subcommands: `list` (alias: `ls`), `log`, `parameters` (alias: `parm`), `module`

```
csr system kernel <SUBCOMMAND>

Subcommands:
  list        List kernel modules                (alias: ls)
  log         Show kernel log (dmesg)
  parameters  Show kernel parameters             (alias: parm)
  module      Module operations
```

```sh
csr system kernel list
csr system kernel ls
csr system kernel log -f -l 100
csr system kernel log --follow --lines 200 -L warning
csr system kernel parameters
csr system kernel parm -g vm
csr system kernel module -n usb-storage -l
csr system kernel module -n usb-storage -u
```

#### `system env`

| Flag | Short | Long | Type | Default | Description |
|------|-------|------|------|---------|-------------|
| set | `-s` | `--set` | `Option<String>` | — | Set environment variable |
| get | `-g` | `--get` | `Option<String>` | — | Get environment variable |
| unset | `-u` | `--unset` | `Option<String>` | — | Unset environment variable |
| list | `-l` | `--list` | `bool` | `false` | List all |
| json | `-j` | `--json` | `bool` | `false` | Output as JSON |

```sh
csr system env -l
csr system env -g PATH
csr system env -s MY_VAR=hello
csr system env -u MY_VAR
csr system env --list --json
```

#### `system resource`

| Flag | Short | Long | Type | Default | Description |
|------|-------|------|------|---------|-------------|
| type | `-t` | `--type` | `Option<String>` | — | Resource type (cpu/memory/io/network) |
| interval | `-i` | `--interval` | `Option<u64>` | — | Snapshot interval |
| format | `-f` | `--format` | `Option<String>` | — | Output format |
| json | `-j` | `--json` | `bool` | `false` | Output as JSON |

```sh
csr system resource
csr system resource -t memory
csr system resource -t cpu -i 5
csr system resource -j
```

#### `system cgroup`

| Flag | Short | Long | Type | Default | Description |
|------|-------|------|------|---------|-------------|
| list | `-l` | `--list` | `bool` | `false` | List cgroups |
| create | `-c` | `--create` | `Option<String>` | — | Create cgroup |
| destroy | `-d` | `--destroy` | `Option<String>` | — | Destroy cgroup |
| attach | `-a` | `--attach` | `Option<String>` | — | Attach PID to cgroup |
| stats | `-s` | `--stats` | `Option<String>` | — | Show cgroup stats |
| pids | `-p` | `--pids` | `Option<String>` | — | PIDs to attach |

```sh
csr system cgroup -l
csr system cgroup -c my-cgroup
csr system cgroup -a my-cgroup -p 1234
csr system cgroup -s my-cgroup
csr system cgroup -d my-cgroup
```

#### `system device`

| Flag | Short | Long | Type | Default | Description |
|------|-------|------|------|---------|-------------|
| list | `-l` | `--list` | `bool` | `false` | List devices |
| attach | `-a` | `--attach` | `Option<String>` | — | Attach device |
| detach | `-d` | `--detach` | `Option<String>` | — | Detach device |
| info | `-i` | `--info` | `Option<String>` | — | Device info |

```sh
csr system device -l
csr system device -i sda
csr system device -a /dev/sdb
csr system device -d /dev/sdb
```

---

### config

Configuration management. Alias: `cfg`

```
csr config <COMMAND>

Commands:
  show      Show configuration
  get       Get a config value
  set       Set a config value
  edit      Edit configuration file
  diff      Diff configurations
  validate  Validate configuration
  import    Import configuration
  export    Export configuration
  backup    Backup configuration
  restore   Restore configuration
  schema    Generate schema
  migrate   Migrate configuration
```

#### `config show`

| Flag | Short | Long | Type | Default | Description |
|------|-------|------|------|---------|-------------|
| format | `-f` | `--format` | `Option<String>` | — | Output format |
| file | `-F` | `--file` | `Option<String>` | — | Config file path |
| section | `-s` | `--section` | `Option<String>` | — | Section filter |

```sh
csr config show
csr config show -s boot
csr config show -F /etc/cesar/services/udevd.ini
csr config show --format json
```

#### `config get`

| Flag | Short | Long | Type | Default | Description |
|------|-------|------|------|---------|-------------|
| key | `-k` | `--key` | `String` | — | Key to get (required) |
| default | `-d` | `--default` | `Option<String>` | — | Default value |

```sh
csr config get -k boot.timeout
csr config get --key boot.timeout --default 30
```

#### `config set`

| Flag | Short | Long | Type | Default | Description |
|------|-------|------|------|---------|-------------|
| key | `-k` | `--key` | `String` | — | Key to set (required) |
| value | `-v` | `--value` | `String` | — | Value to set (required) |
| file | `-F` | `--file` | `Option<String>` | — | Config file |

```sh
csr config set -k boot.timeout -v 60
csr config set --key boot.verbose --value true -F /etc/cesar/config.ini
```

#### `config edit`

| Flag | Short | Long | Type | Default | Description |
|------|-------|------|------|---------|-------------|
| file | `-F` | `--file` | `Option<String>` | — | Config file |
| editor | `-e` | `--editor` | `Option<String>` | — | Editor to use |
| validate | `-V` | `--validate` | `bool` | `false` | Validate after edit |

```sh
csr config edit
csr config edit -F /etc/cesar/services/greetd.ini
csr config edit -e vim -V
```

#### `config diff`

| Flag | Short | Long | Type | Default | Description |
|------|-------|------|------|---------|-------------|
| file | `-F` | `--file` | `Option<String>` | — | Config file |
| target | `-t` | `--target` | `Option<String>` | — | Target to compare |
| context | `-c` | `--context` | `Option<usize>` | — | Context lines |

```sh
csr config diff -F /etc/cesar/services/greetd.ini -t /tmp/greetd.ini
csr config diff --context 5
```

#### `config validate`

| Flag | Short | Long | Type | Default | Description |
|------|-------|------|------|---------|-------------|
| file | `-F` | `--file` | `Option<String>` | — | Config file |
| schema | `-s` | `--schema` | `Option<String>` | — | Schema file |
| strict | `-S` | `--strict` | `bool` | `false` | Strict validation |

```sh
csr config validate
csr config validate -F /etc/cesar/services/greetd.ini -S
```

#### `config import`

| Flag | Short | Long | Type | Default | Description |
|------|-------|------|------|---------|-------------|
| file | `-f` | `--file` | `String` | — | File to import (required) |
| format | `-F` | `--format` | `Option<String>` | — | Import format |
| merge | `-m` | `--merge` | `bool` | `false` | Merge with existing |
| overwrite | `-o` | `--overwrite` | `bool` | `false` | Overwrite existing |

```sh
csr config import -f /tmp/backup.ini
csr config import -f /tmp/backup.ini -m -o
```

#### `config export`

| Flag | Short | Long | Type | Default | Description |
|------|-------|------|------|---------|-------------|
| format | `-f` | `--format` | `Option<String>` | — | Export format |
| output | `-o` | `--output` | `Option<String>` | — | Output file |
| all | `-a` | `--all` | `bool` | `false` | Export all configs |
| sections | `-s` | `--sections` | `Option<String>` | — | Sections to export |

```sh
csr config export -a
csr config export -o /tmp/config-backup.ini
csr config export -f json -o /tmp/config.json
```

#### `config backup`

| Flag | Short | Long | Type | Default | Description |
|------|-------|------|------|---------|-------------|
| output | `-o` | `--output` | `Option<String>` | — | Output path |
| compress | `-c` | `--compress` | `bool` | `false` | Compress backup |
| include-logs | `-l` | `--include-logs` | `bool` | `false` | Include logs |

```sh
csr config backup
csr config backup -o /tmp/cesar-backup.tar -c -l
```

#### `config restore`

| Flag | Short | Long | Type | Default | Description |
|------|-------|------|------|---------|-------------|
| file | `-f` | `--file` | `String` | — | Backup file (required) |
| force | `-F` | `--force` | `bool` | `false` | Force restore |
| verify | `-v` | `--verify` | `bool` | `false` | Verify integrity |

```sh
csr config restore -f /tmp/cesar-backup.tar
csr config restore -f /tmp/cesar-backup.tar -F -v
```

#### `config schema`

| Flag | Short | Long | Type | Default | Description |
|------|-------|------|------|---------|-------------|
| generate | `-g` | `--generate` | `bool` | `false` | Generate schema |
| validate | `-v` | `--validate` | `bool` | `false` | Validate against schema |
| format | `-f` | `--format` | `Option<String>` | — | Output format |

```sh
csr config schema -g
csr config schema -g -f json
csr config schema -v
```

#### `config migrate`

| Flag | Short | Long | Type | Default | Description |
|------|-------|------|------|---------|-------------|
| from | `-f` | `--from` | `Option<String>` | — | Source version |
| to | `-t` | `--to` | `Option<String>` | — | Target version |
| dry-run | `-d` | `--dry-run` | `bool` | `false` | Dry run |

```sh
csr config migrate -f 0.0.6 -t 0.0.7
csr config migrate --from 0.0.6 --to 0.0.7 --dry-run
```

---

### log

Unified Markdown log system. Alias: `logs`

```
csr log <COMMAND>

Commands:
  view      View log entries
  tail      Show last N lines
  head      Show first N lines
  grep      Search log entries
  clear     Clear log entries
  rotate    Rotate log files
  archive   Archive log entries
  export    Export log entries
  follow    Follow log in real-time
  errors    Show only errors
  warnings  Show only warnings
  stats     Log statistics
  summary   Log summary
```

#### `log view`

| Flag | Short | Long | Type | Default | Description |
|------|-------|------|------|---------|-------------|
| service | `-s` | `--service` | `Option<String>` | — | Service filter |
| level | `-l` | `--level` | `Option<String>` | — | Level filter |
| grep | `-g` | `--grep` | `Option<String>` | — | Grep pattern |
| reverse | `-r` | `--reverse` | `bool` | `false` | Show in reverse |
| lines | `-n` | `--lines` | `usize` | `100` | Lines limit |
| since | `-S` | `--since` | `Option<String>` | — | Show since timestamp |
| until | `-U` | `--until` | `Option<String>` | — | Show until timestamp |

```sh
csr log view
csr log view -s greetd -l CRITICAL
csr log view -g "not found" -r -n 50
csr log view -S "2026-07-13T00:00:00"
csr log view --service udevd --level WARNING --lines 200
```

#### `log tail`

| Flag | Short | Long | Type | Default | Description |
|------|-------|------|------|---------|-------------|
| lines | `-n` | `--lines` | `usize` | `50` | Number of lines |
| service | `-s` | `--service` | `Option<String>` | — | Service filter |
| follow | `-f` | `--follow` | `bool` | `false` | Follow mode |
| sleep | `-S` | `--sleep` | `u64` | `1` | Sleep interval for follow |

```sh
csr log tail
csr log tail -n 100
csr log tail -s greetd -f
csr log tail --lines 200 --follow --sleep 2
```

#### `log head`

| Flag | Short | Long | Type | Default | Description |
|------|-------|------|------|---------|-------------|
| lines | `-n` | `--lines` | `usize` | `50` | Number of lines |
| service | `-s` | `--service` | `Option<String>` | — | Service filter |

```sh
csr log head
csr log head -n 20
csr log head -s udevd
```

#### `log grep`

| Flag | Short | Long | Type | Default | Description |
|------|-------|------|------|---------|-------------|
| pattern | `-p` | `--pattern` | `String` | — | Search pattern (required) |
| service | `-s` | `--service` | `Option<String>` | — | Service filter |
| level | `-l` | `--level` | `Option<String>` | — | Level filter |
| context | `-c` | `--context` | `Option<usize>` | — | Context lines |
| count | `-C` | `--count` | `bool` | `false` | Count only |

```sh
csr log grep -p "not found"
csr log grep -p "CRITICAL" -s greetd -c 3
csr log grep -p "timeout" -C
```

#### `log clear`

| Flag | Short | Long | Type | Default | Description |
|------|-------|------|------|---------|-------------|
| service | `-s` | `--service` | `Option<String>` | — | Service filter |
| before | `-b` | `--before` | `Option<String>` | — | Clear entries before timestamp |
| yes | `-y` | `--yes` | `bool` | `false` | Skip confirmation |

```sh
csr log clear -y
csr log clear -s udevd -y
csr log clear -b "2026-07-13T00:00:00" -y
```

#### `log rotate`

| Flag | Short | Long | Type | Default | Description |
|------|-------|------|------|---------|-------------|
| max-size | `-s` | `--max-size` | `usize` | `10` | Max file size in MB |
| compress | `-c` | `--compress` | `bool` | `false` | Compress rotated logs |
| archive | `-a` | `--archive` | `Option<String>` | — | Archive directory |
| keep | `-k` | `--keep` | `usize` | `5` | Keep N rotated files |

```sh
csr log rotate
csr log rotate -s 20 -c -k 10
csr log rotate --max-size 50 --compress --archive /var/log/cesar/
```

#### `log archive`

| Flag | Short | Long | Type | Default | Description |
|------|-------|------|------|---------|-------------|
| format | `-f` | `--format` | `Option<String>` | — | Output format |
| output | `-o` | `--output` | `Option<String>` | — | Output file |
| since | `-S` | `--since` | `Option<String>` | — | Since timestamp |
| until | `-U` | `--until` | `Option<String>` | — | Until timestamp |
| compress | `-c` | `--compress` | `bool` | `false` | Compress archive |

```sh
csr log archive -o /tmp/cesar-logs.tar
csr log archive -S "2026-07-01" -U "2026-07-13" -c
```

#### `log export`

| Flag | Short | Long | Type | Default | Description |
|------|-------|------|------|---------|-------------|
| format | `-f` | `--format` | `Option<String>` | — | Output format |
| output | `-o` | `--output` | `Option<String>` | — | Output file |
| service | `-s` | `--service` | `Option<String>` | — | Service filter |
| level | `-l` | `--level` | `Option<String>` | — | Level filter |

```sh
csr log export -f json -o /tmp/cesar-logs.json
csr log export -s greetd -l CRITICAL -o /tmp/greetd-errors.md
```

#### `log follow`

| Flag | Short | Long | Type | Default | Description |
|------|-------|------|------|---------|-------------|
| service | `-s` | `--service` | `Option<String>` | — | Service filter |
| level | `-l` | `--level` | `Option<String>` | — | Level filter |
| grep | `-g` | `--grep` | `Option<String>` | — | Grep pattern |
| sleep | `-S` | `--sleep` | `u64` | `1` | Sleep interval |

```sh
csr log follow
csr log follow -s greetd -l CRITICAL
csr log follow -g "error" -S 2
```

#### `log errors`

| Flag | Short | Long | Type | Default | Description |
|------|-------|------|------|---------|-------------|
| service | `-s` | `--service` | `Option<String>` | — | Service filter |
| context | `-c` | `--context` | `Option<usize>` | — | Context lines |
| group | `-g` | `--group` | `bool` | `false` | Group by service |
| top | `-t` | `--top` | `Option<usize>` | — | Top N errors |

```sh
csr log errors
csr log errors -s greetd -c 3
csr log errors -g -t 10
```

#### `log warnings`

| Flag | Short | Long | Type | Default | Description |
|------|-------|------|------|---------|-------------|
| service | `-s` | `--service` | `Option<String>` | — | Service filter |
| context | `-c` | `--context` | `Option<usize>` | — | Context lines |
| group | `-g` | `--group` | `bool` | `false` | Group by service |
| top | `-t` | `--top` | `Option<usize>` | — | Top N warnings |

```sh
csr log warnings
csr log warnings -g -t 5
```

#### `log stats`

| Flag | Short | Long | Type | Default | Description |
|------|-------|------|------|---------|-------------|
| service | `-s` | `--service` | `Option<String>` | — | Service filter |
| period | `-p` | `--period` | `Option<String>` | — | Time period |
| format | `-f` | `--format` | `Option<String>` | — | Output format |

```sh
csr log stats
csr log stats -s greetd -p 24h
```

#### `log summary`

| Flag | Short | Long | Type | Default | Description |
|------|-------|------|------|---------|-------------|
| since | `-S` | `--since` | `Option<String>` | — | Since timestamp |
| until | `-U` | `--until` | `Option<String>` | — | Until timestamp |
| format | `-f` | `--format` | `Option<String>` | — | Output format |
| top | `-t` | `--top` | `Option<usize>` | — | Top N entries |

```sh
csr log summary
csr log summary -S "2026-07-13" -t 10
```

---

### socket

Sockets and monitoring. Alias: `sock`

```
csr socket <COMMAND>

Commands:
  list      List sockets         (alias: ls)
  status    Show socket status
  create    Create a socket
  destroy   Destroy a socket
  monitor   Monitor socket activity
  trace     Trace socket events
  activate  Activate a socket
  query     Query socket
```

#### `socket list`

| Flag | Short | Long | Type | Default | Description |
|------|-------|------|------|---------|-------------|
| type | `-t` | `--type` | `Option<String>` | — | Socket type filter |
| state | `-s` | `--state` | `Option<String>` | — | State filter |
| service | `-S` | `--service` | `Option<String>` | — | Service filter |
| json | `-j` | `--json` | `bool` | `false` | Output as JSON |

```sh
csr socket list
csr socket ls -t unix
csr socket list -s listening -S greetd -j
```

#### `socket status`

| Flag | Short | Long | Type | Default | Description |
|------|-------|------|------|---------|-------------|
| path | `-p` | `--path` | `Option<String>` | — | Socket path |
| verbose | `-v` | `--verbose` | `bool` | `false` | Verbose output |

```sh
csr socket status -p /run/cesar.sock
csr socket status -p /run/cesar.sock -v
```

#### `socket create`

| Flag | Short | Long | Type | Default | Description |
|------|-------|------|------|---------|-------------|
| path | `-p` | `--path` | `String` | — | Socket path (required) |
| type | `-t` | `--type` | `Option<String>` | — | Socket type |
| backlog | `-b` | `--backlog` | `i32` | `128` | Backlog size |
| service | `-s` | `--service` | `Option<String>` | — | Associated service |

```sh
csr socket create -p /run/my-svc.sock
csr socket create -p /run/my-svc.sock -t unix -b 256 -s my-svc
```

#### `socket destroy`

| Flag | Short | Long | Type | Default | Description |
|------|-------|------|------|---------|-------------|
| path | `-p` | `--path` | `String` | — | Socket path (required) |
| force | `-f` | `--force` | `bool` | `false` | Force destruction |

```sh
csr socket destroy -p /run/my-svc.sock
csr socket destroy -p /run/my-svc.sock -f
```

#### `socket monitor`

| Flag | Short | Long | Type | Default | Description |
|------|-------|------|------|---------|-------------|
| path | `-p` | `--path` | `String` | — | Socket path (required) |
| interval | `-i` | `--interval` | `u64` | `1000` | Poll interval |
| events | `-e` | `--events` | `Option<String>` | — | Event types |

```sh
csr socket monitor -p /run/cesar.sock
csr socket monitor -p /run/cesar.sock -i 500 -e connect,disconnect
```

#### `socket trace`

| Flag | Short | Long | Type | Default | Description |
|------|-------|------|------|---------|-------------|
| path | `-p` | `--path` | `String` | — | Socket path (required) |
| duration | `-d` | `--duration` | `Option<u64>` | — | Duration in seconds |
| filter | `-f` | `--filter` | `Option<String>` | — | Filter expression |

```sh
csr socket trace -p /run/cesar.sock
csr socket trace -p /run/cesar.sock -d 30 -f "type == connect"
```

#### `socket activate`

| Flag | Short | Long | Type | Default | Description |
|------|-------|------|------|---------|-------------|
| path | `-p` | `--path` | `String` | — | Socket path (required) |
| service | `-s` | `--service` | `Option<String>` | — | Service to activate |
| one-shot | `-o` | `--one-shot` | `bool` | `false` | One-shot mode |

```sh
csr socket activate -p /run/cesar.sock
csr socket activate -p /run/cesar.sock -s my-svc -o
```

#### `socket query`

| Flag | Short | Long | Type | Default | Description |
|------|-------|------|------|---------|-------------|
| path | `-p` | `--path` | `String` | — | Socket path (required) |
| data | `-d` | `--data` | `Option<String>` | — | Data to send |
| timeout | `-t` | `--timeout` | `u64` | `5` | Timeout |
| non-blocking | `-n` | `--non-blocking` | `bool` | `false` | Non-blocking mode |

```sh
csr socket query -p /run/cesar.sock
csr socket query -p /run/cesar.sock -d "hello" -t 10
csr socket query -p /run/cesar.sock -d "ping" -n
```

---

### daemon

Daemon process management. Alias: `dmon`

```
csr daemon <COMMAND>

Commands:
  start      Start a daemon
  stop       Stop a daemon
  restart    Restart a daemon
  status     Daemon status
  list       List daemons       (alias: ls)
  log        Daemon logs
  install    Install a daemon
  uninstall  Uninstall a daemon
  update     Update a daemon
  rollback   Rollback a daemon
  pin        Pin daemon version
  trust      Trust management
```

#### `daemon start`

| Flag | Short | Long | Type | Default | Description |
|------|-------|------|------|---------|-------------|
| name | `-n` | `--name` | `String` | — | Daemon name (required) |
| wait | `-w` | `--wait` | `bool` | `false` | Wait for daemon |
| timeout | `-t` | `--timeout` | `u64` | `30` | Timeout in seconds |

```sh
csr daemon start -n my-daemon
csr daemon start -n my-daemon -w -t 60
```

#### `daemon stop`

| Flag | Short | Long | Type | Default | Description |
|------|-------|------|------|---------|-------------|
| name | `-n` | `--name` | `String` | — | Daemon name (required) |
| force | `-f` | `--force` | `bool` | `false` | Force stop |
| timeout | `-t` | `--timeout` | `u64` | `30` | Timeout in seconds |

```sh
csr daemon stop -n my-daemon
csr daemon stop -n my-daemon -f -t 10
```

#### `daemon restart`

| Flag | Short | Long | Type | Default | Description |
|------|-------|------|------|---------|-------------|
| name | `-n` | `--name` | `String` | — | Daemon name (required) |
| force | `-f` | `--force` | `bool` | `false` | Force restart |
| timeout | `-t` | `--timeout` | `u64` | `30` | Timeout in seconds |

```sh
csr daemon restart -n my-daemon
csr daemon restart -n my-daemon -f
```

#### `daemon status`

| Flag | Short | Long | Type | Default | Description |
|------|-------|------|------|---------|-------------|
| name | `-n` | `--name` | `Option<String>` | — | Daemon name |
| all | `-a` | `--all` | `bool` | `false` | Show all |
| json | `-j` | `--json` | `bool` | `false` | Output as JSON |

```sh
csr daemon status
csr daemon status -n my-daemon
csr daemon status -a -j
```

#### `daemon list`

| Flag | Short | Long | Type | Default | Description |
|------|-------|------|------|---------|-------------|
| all | `-a` | `--all` | `bool` | `false` | Show all |
| running | `-r` | `--running` | `bool` | `false` | Show only running |
| failed | `-f` | `--failed` | `bool` | `false` | Show only failed |
| json | `-j` | `--json` | `bool` | `false` | Output as JSON |

```sh
csr daemon list
csr daemon ls -a -j
csr daemon list -r -f
```

#### `daemon log`

| Flag | Short | Long | Type | Default | Description |
|------|-------|------|------|---------|-------------|
| name | `-n` | `--name` | `String` | — | Daemon name (required) |
| follow | `-f` | `--follow` | `bool` | `false` | Follow log |
| lines | `-l` | `--lines` | `usize` | `50` | Number of lines |

```sh
csr daemon log -n my-daemon
csr daemon log -n my-daemon -f -l 100
```

#### `daemon install`

| Flag | Short | Long | Type | Default | Description |
|------|-------|------|------|---------|-------------|
| name | `-n` | `--name` | `String` | — | Daemon name (required) |
| from | `-f` | `--from` | `Option<String>` | — | Source |
| enable | `-e` | `--enable` | `bool` | `false` | Enable after install |
| start | `-s` | `--start` | `bool` | `false` | Start after install |

```sh
csr daemon install -n my-daemon
csr daemon install -n my-daemon -f /opt/my-daemon -e -s
```

#### `daemon uninstall`

| Flag | Short | Long | Type | Default | Description |
|------|-------|------|------|---------|-------------|
| name | `-n` | `--name` | `String` | — | Daemon name (required) |
| stop | `-s` | `--stop` | `bool` | `false` | Stop before uninstall |
| purge | `-p` | `--purge` | `bool` | `false` | Purge all data |

```sh
csr daemon uninstall -n my-daemon
csr daemon uninstall -n my-daemon -s -p
```

#### `daemon update`

| Flag | Short | Long | Type | Default | Description |
|------|-------|------|------|---------|-------------|
| name | `-n` | `--name` | `String` | — | Daemon name (required) |
| from | `-f` | `--from` | `Option<String>` | — | Source |
| force | `-F` | `--force` | `bool` | `false` | Force update |

```sh
csr daemon update -n my-daemon
csr daemon update -n my-daemon -f /opt/new-version -F
```

#### `daemon rollback`

| Flag | Short | Long | Type | Default | Description |
|------|-------|------|------|---------|-------------|
| name | `-n` | `--name` | `String` | — | Daemon name (required) |
| to-revision | `-r` | `--to-revision` | `Option<String>` | — | Target revision |

```sh
csr daemon rollback -n my-daemon
csr daemon rollback -n my-daemon -r v1.0.0
```

#### `daemon pin`

| Flag | Short | Long | Type | Default | Description |
|------|-------|------|------|---------|-------------|
| name | `-n` | `--name` | `String` | — | Daemon name (required) |
| version | `-v` | `--version` | `Option<String>` | — | Version to pin |

```sh
csr daemon pin -n my-daemon -v 1.0.0
```

#### `daemon trust`

| Flag | Short | Long | Type | Default | Description |
|------|-------|------|------|---------|-------------|
| name | `-n` | `--name` | `String` | — | Daemon name (required) |
| key | `-k` | `--key` | `Option<String>` | — | Trust key |
| verify | `-v` | `--verify` | `bool` | `false` | Verify trust |

```sh
csr daemon trust -n my-daemon -k /path/to/key.pub
csr daemon trust -n my-daemon -v
```

---

### snapshot

System snapshots and state management. Alias: `snap`

```
csr snapshot <COMMAND>

Commands:
  create   Create a snapshot
  list     List snapshots       (alias: ls)
  restore  Restore a snapshot
  delete   Delete a snapshot
  diff     Diff two snapshots
  export   Export a snapshot
  import   Import a snapshot
```

#### `snapshot create`

| Flag | Short | Long | Type | Default | Description |
|------|-------|------|------|---------|-------------|
| name | `-n` | `--name` | `String` | — | Snapshot name (required) |
| description | `-d` | `--description` | `Option<String>` | — | Description |
| services | `-s` | `--services` | `Option<String>` | — | Services to include |
| include-logs | `-l` | `--include-logs` | `bool` | `false` | Include logs |

```sh
csr snapshot create -n before-update
csr snapshot create -n before-update -d "Before major update" -s greetd,pipewire -l
```

#### `snapshot list`

| Flag | Short | Long | Type | Default | Description |
|------|-------|------|------|---------|-------------|
| all | `-a` | `--all` | `bool` | `false` | Show all |
| recent | `-r` | `--recent` | `bool` | `false` | Show recent only |
| json | `-j` | `--json` | `bool` | `false` | Output as JSON |

```sh
csr snapshot list
csr snapshot ls -a -j
csr snapshot list -r
```

#### `snapshot restore`

| Flag | Short | Long | Type | Default | Description |
|------|-------|------|------|---------|-------------|
| name | `-n` | `--name` | `String` | — | Snapshot name (required) |
| force | `-f` | `--force` | `bool` | `false` | Force restore |
| dry-run | `-d` | `--dry-run` | `bool` | `false` | Dry run |

```sh
csr snapshot restore -n before-update
csr snapshot restore -n before-update -f
csr snapshot restore -n before-update -d
```

#### `snapshot delete`

| Flag | Short | Long | Type | Default | Description |
|------|-------|------|------|---------|-------------|
| name | `-n` | `--name` | `String` | — | Snapshot name (required) |
| force | `-f` | `--force` | `bool` | `false` | Force deletion |

```sh
csr snapshot delete -n before-update
csr snapshot delete -n before-update -f
```

#### `snapshot diff`

| Flag | Short | Long | Type | Default | Description |
|------|-------|------|------|---------|-------------|
| from | `-f` | `--from` | `String` | — | From snapshot (required) |
| to | `-t` | `--to` | `String` | — | To snapshot (required) |
| services | `-s` | `--services` | `Option<String>` | — | Services to compare |

```sh
csr snapshot diff -f before-update -t latest
csr snapshot diff --from before-update --to latest --services greetd,pipewire
```

#### `snapshot export`

| Flag | Short | Long | Type | Default | Description |
|------|-------|------|------|---------|-------------|
| name | `-n` | `--name` | `String` | — | Snapshot name (required) |
| output | `-o` | `--output` | `String` | — | Output file (required) |
| format | `-f` | `--format` | `Option<String>` | — | Export format |

```sh
csr snapshot export -n before-update -o /tmp/snapshot.tar
csr snapshot export -n before-update -o /tmp/snapshot.json -f json
```

#### `snapshot import`

| Flag | Short | Long | Type | Default | Description |
|------|-------|------|------|---------|-------------|
| file | `-f` | `--file` | `String` | — | Import file (required) |
| name | `-n` | `--name` | `Option<String>` | — | Snapshot name |
| force | `-F` | `--force` | `bool` | `false` | Force import |

```sh
csr snapshot import -f /tmp/snapshot.tar
csr snapshot import -f /tmp/snapshot.tar -n my-snapshot -F
```

---

### security

Security auditing and policy enforcement. Alias: `sec`

```
csr security <COMMAND>

Commands:
  audit    Run security audit
  scan     Security scan
  policy   Security policy
  cap      Capability management
  seccomp  Seccomp filter management
  sandbox  Sandbox management
  trust    Trust key management
```

#### `security audit`

| Flag | Short | Long | Type | Default | Description |
|------|-------|------|------|---------|-------------|
| all | `-a` | `--all` | `bool` | `false` | Audit all |
| services | `-s` | `--services` | `bool` | `false` | Audit services |
| config | `-c` | `--config` | `bool` | `false` | Audit config |
| network | `-n` | `--network` | `bool` | `false` | Audit network |

```sh
csr security audit -a
csr security audit -s -c -n
csr security audit --services --config
```

#### `security scan`

| Flag | Short | Long | Type | Default | Description |
|------|-------|------|------|---------|-------------|
| targets | `-t` | `--targets` | `Option<String>` | — | Scan targets |
| severity | `-s` | `--severity` | `Option<String>` | — | Severity filter |
| fix | `-f` | `--fix` | `bool` | `false` | Auto-fix issues |

```sh
csr security scan
csr security scan -t /usr/bin -s critical
csr security scan -f
```

#### `security policy`

| Flag | Short | Long | Type | Default | Description |
|------|-------|------|------|---------|-------------|
| show | `-s` | `--show` | `bool` | `false` | Show current policy |
| set | `-S` | `--set` | `Option<String>` | — | Set policy |
| validate | `-v` | `--validate` | `bool` | `false` | Validate policy |

```sh
csr security policy -s
csr security policy -S strict
csr security policy -v
```

#### `security cap`

| Flag | Short | Long | Type | Default | Description |
|------|-------|------|------|---------|-------------|
| list | `-l` | `--list` | `bool` | `false` | List capabilities |
| add | `-a` | `--add` | `Option<String>` | — | Add capability |
| drop | `-d` | `--drop` | `Option<String>` | — | Drop capability |
| pid | `-p` | `--pid` | `Option<u32>` | — | Target PID |

```sh
csr security cap -l
csr security cap -a cap_net_bind_service -p 1234
csr security cap -d cap_sys_admin -p 1234
```

#### `security seccomp`

| Flag | Short | Long | Type | Default | Description |
|------|-------|------|------|---------|-------------|
| list | `-l` | `--list` | `bool` | `false` | List filters |
| apply | `-a` | `--apply` | `Option<String>` | — | Apply filter |
| dump | `-d` | `--dump` | `Option<String>` | — | Dump filter |
| pid | `-p` | `--pid` | `Option<u32>` | — | Target PID |

```sh
csr security seccomp -l
csr security seccomp -a strict -p 1234
csr security seccomp -d default
```

#### `security sandbox`

| Flag | Short | Long | Type | Default | Description |
|------|-------|------|------|---------|-------------|
| create | `-c` | `--create` | `Option<String>` | — | Create sandbox |
| destroy | `-d` | `--destroy` | `Option<String>` | — | Destroy sandbox |
| list | `-l` | `--list` | `bool` | `false` | List sandboxes |
| info | `-i` | `--info` | `Option<String>` | — | Sandbox info |

```sh
csr security sandbox -l
csr security sandbox -c my-sandbox
csr security sandbox -i my-sandbox
csr security sandbox -d my-sandbox
```

#### `security trust`

| Flag | Short | Long | Type | Default | Description |
|------|-------|------|------|---------|-------------|
| keys | `-k` | `--keys` | `bool` | `false` | List trust keys |
| add | `-a` | `--add` | `Option<String>` | — | Add trust key |
| remove | `-r` | `--remove` | `Option<String>` | — | Remove trust key |
| verify | `-v` | `--verify` | `bool` | `false` | Verify trust |

```sh
csr security trust -k
csr security trust -a /path/to/key.pub
csr security trust -r my-key
csr security trust -v
```

---

### query

Query system state and information. Alias: `qry`

```
csr query <COMMAND>

Commands:
  service     Query service information
  boot        Query boot information
  system      Query system information
  dependency  Query dependencies
  history     Query history
  resource    Query resource usage
  health      Health check
```

#### `query service`

| Flag | Short | Long | Type | Default | Description |
|------|-------|------|------|---------|-------------|
| name | `-n` | `--name` | `Option<String>` | — | Service name |
| format | `-f` | `--format` | `Option<String>` | — | Output format |
| fields | `-F` | `--fields` | `Option<String>` | — | Fields to show |
| json | `-j` | `--json` | `bool` | `false` | Output as JSON |

```sh
csr query service
csr query service -n greetd
csr query service -n greetd -F name,pid,state -j
```

#### `query boot`

| Flag | Short | Long | Type | Default | Description |
|------|-------|------|------|---------|-------------|
| phase | `-p` | `--phase` | `Option<String>` | — | Boot phase |
| time | `-t` | `--time` | `bool` | `false` | Show boot time |
| errors | `-e` | `--errors` | `bool` | `false` | Show boot errors |
| format | `-f` | `--format` | `Option<String>` | — | Output format |
| json | `-j` | `--json` | `bool` | `false` | Output as JSON |

```sh
csr query boot
csr query boot -t -e
csr query boot -p services -j
```

#### `query system`

| Flag | Short | Long | Type | Default | Description |
|------|-------|------|------|---------|-------------|
| all | `-a` | `--all` | `bool` | `false` | Show all |
| format | `-f` | `--format` | `Option<String>` | — | Output format |
| fields | `-F` | `--fields` | `Option<String>` | — | Fields to show |
| json | `-j` | `--json` | `bool` | `false` | Output as JSON |

```sh
csr query system
csr query system -a -j
csr query system -F hostname,uptime,arch
```

#### `query dependency`

| Flag | Short | Long | Type | Default | Description |
|------|-------|------|------|---------|-------------|
| name | `-n` | `--name` | `Option<String>` | — | Service name |
| reverse | `-r` | `--reverse` | `bool` | `false` | Show reverse deps |
| format | `-f` | `--format` | `Option<String>` | — | Output format |
| graph | `-g` | `--graph` | `bool` | `false` | Show as graph |

```sh
csr query dependency
csr query dependency -n greetd
csr query dependency -n greetd -r -g
```

#### `query history`

| Flag | Short | Long | Type | Default | Description |
|------|-------|------|------|---------|-------------|
| limit | `-l` | `--limit` | `usize` | `20` | Limit entries |
| format | `-f` | `--format` | `Option<String>` | — | Output format |
| events | `-e` | `--events` | `Option<String>` | — | Event filter |
| json | `-j` | `--json` | `bool` | `false` | Output as JSON |

```sh
csr query history
csr query history -l 50 -e start,stop
csr query history -j
```

#### `query resource`

| Flag | Short | Long | Type | Default | Description |
|------|-------|------|------|---------|-------------|
| type | `-t` | `--type` | `Option<String>` | — | Resource type |
| interval | `-i` | `--interval` | `Option<u64>` | — | Snapshot interval |
| format | `-f` | `--format` | `Option<String>` | — | Output format |
| json | `-j` | `--json` | `bool` | `false` | Output as JSON |

```sh
csr query resource
csr query resource -t memory -i 5 -j
```

#### `query health`

| Flag | Short | Long | Type | Default | Description |
|------|-------|------|------|---------|-------------|
| all | `-a` | `--all` | `bool` | `false` | Check all |
| service | `-s` | `--service` | `Option<String>` | — | Service to check |
| format | `-f` | `--format` | `Option<String>` | — | Output format |
| json | `-j` | `--json` | `bool` | `false` | Output as JSON |

```sh
csr query health -a
csr query health -s greetd -j
```

---

### debug

Debugging and diagnostic tools. Alias: `dbg`

```
csr debug <COMMAND>

Commands:
  trace    Trace a process
  strace   Strace-like tracing
  dump     Dump system state
  core     Core dump analysis
  profile  Profile a process
  stress   Stress test
  test     Run self-tests
```

#### `debug trace`

| Flag | Short | Long | Type | Default | Description |
|------|-------|------|------|---------|-------------|
| pid | `-p` | `--pid` | `u32` | — | Process ID (required) |
| signal | `-s` | `--signal` | `Option<String>` | — | Signal to send |
| output | `-o` | `--output` | `Option<String>` | — | Output file |

```sh
csr debug trace -p 1234
csr debug trace -p 1234 -s SIGUSR1 -o /tmp/trace.log
```

#### `debug strace`

| Flag | Short | Long | Type | Default | Description |
|------|-------|------|------|---------|-------------|
| pid | `-p` | `--pid` | `u32` | — | Process ID (required) |
| filter | `-f` | `--filter` | `Option<String>` | — | Filter expression |
| output | `-o` | `--output` | `Option<String>` | — | Output file |
| timeout | `-t` | `--timeout` | `Option<u64>` | — | Timeout in seconds |

```sh
csr debug strace -p 1234
csr debug strace -p 1234 -f "open,read,write" -o /tmp/strace.log -t 30
```

#### `debug dump`

| Flag | Short | Long | Type | Default | Description |
|------|-------|------|------|---------|-------------|
| all | `-a` | `--all` | `bool` | `false` | Dump all |
| services | `-s` | `--services` | `bool` | `false` | Dump services |
| format | `-f` | `--format` | `Option<String>` | — | Output format |
| output | `-o` | `--output` | `Option<String>` | — | Output file |

```sh
csr debug dump -a
csr debug dump -s -f json -o /tmp/state.json
```

#### `debug core`

| Flag | Short | Long | Type | Default | Description |
|------|-------|------|------|---------|-------------|
| pid | `-p` | `--pid` | `u32` | — | Process ID (required) |
| output | `-o` | `--output` | `Option<String>` | — | Output file |
| limit | `-l` | `--limit` | `Option<usize>` | — | Output limit |

```sh
csr debug core -p 1234
csr debug core -p 1234 -o /tmp/core.log -l 500
```

#### `debug profile`

| Flag | Short | Long | Type | Default | Description |
|------|-------|------|------|---------|-------------|
| pid | `-p` | `--pid` | `u32` | — | Process ID (required) |
| duration | `-d` | `--duration` | `u64` | `10` | Duration in seconds |
| frequency | `-f` | `--frequency` | `u32` | `99` | Sampling frequency |
| output | `-o` | `--output` | `Option<String>` | — | Output file |

```sh
csr debug profile -p 1234
csr debug profile -p 1234 -d 30 -f 999 -o /tmp/profile.folded
```

#### `debug stress`

| Flag | Short | Long | Type | Default | Description |
|------|-------|------|------|---------|-------------|
| cpu | `-c` | `--cpu` | `bool` | `false` | CPU stress |
| memory | `-m` | `--memory` | `bool` | `false` | Memory stress |
| io | `-i` | `--io` | `bool` | `false` | I/O stress |
| duration | `-d` | `--duration` | `u64` | `30` | Duration in seconds |

```sh
csr debug stress -c -m -d 60
csr debug stress --io --duration 120
```

#### `debug test`

| Flag | Short | Long | Type | Default | Description |
|------|-------|------|------|---------|-------------|
| module | `-m` | `--module` | `Option<String>` | — | Module to test |
| all | `-a` | `--all` | `bool` | `false` | Run all tests |
| verbose | `-v` | `--verbose` | `bool` | `false` | Verbose output |

```sh
csr debug test -a
csr debug test -m config -v
csr debug test --all --verbose
```

---

### self

Cesar self-management. Alias: `me`

```
csr self <COMMAND>

Commands:
  status       Cesar status
  update       Self-update
  version      Version information
  completions  Generate completions
  config       Cesar configuration
```

#### `self status`

Shows PID, role, uptime, and runs built-in health checks (kernel, filesystems, disk, service configs, zombies, TCP listeners).

| Flag | Short | Long | Type | Default | Description |
|------|-------|------|------|---------|-------------|
| verbose | `-v` | `--verbose` | `bool` | `false` | Verbose output |

```sh
csr self status
csr self status -v
```

**Health checks run automatically:**
- On boot: silently, issues logged to `/var/log/cesar.md`
- On `self status`: displayed inline with `[CTX]` and `[FIX]` for each issue

#### `self update`

| Flag | Short | Long | Type | Default | Description |
|------|-------|------|------|---------|-------------|
| force | `-f` | `--force` | `bool` | `false` | Force update |
| check | `-c` | `--check` | `bool` | `false` | Check only (don't update) |
| channel | `-C` | `--channel` | `Option<String>` | — | Update channel |

```sh
csr self update
csr self update -f
csr self update -c
csr self update --channel stable
```

#### `self version`

| Flag | Short | Long | Type | Default | Description |
|------|-------|------|------|---------|-------------|
| json | `-j` | `--json` | `bool` | `false` | Output as JSON |
| short | `-s` | `--short` | `bool` | `false` | Short output |

```sh
csr self version
csr self version -j
csr self version -s
```

#### `self completions`

| Flag | Short | Long | Type | Default | Description |
|------|-------|------|------|---------|-------------|
| shell | `-s` | `--shell` | `Option<String>` | — | Shell type (bash/zsh/fish) |
| output | `-o` | `--output` | `Option<String>` | — | Output file |

```sh
csr self completions -s bash
csr self completions -s zsh -o ~/.zsh/completions/_cs
csr self completions -s fish -o ~/.config/fish/completions/csr.fish
```

#### `self config`

| Flag | Short | Long | Type | Default | Description |
|------|-------|------|------|---------|-------------|
| show | `-s` | `--show` | `bool` | `false` | Show configuration |
| set | `-S` | `--set` | `Option<String>` | — | Set configuration |
| get | `-g` | `--get` | `Option<String>` | — | Get configuration |

```sh
csr self config -s
csr self config -g boot.verbose
csr self config -S boot.verbose=true
```

---

</details>

<details>
<summary>CLI</summary>

```
csr
├── service (svc)
│   ├── start    (up)    -n NAME [-w] [-t SEC]
│   ├── stop     (dn)    -n NAME [-f] [-t SEC]
│   ├── restart  (rs)    -n NAME [-f] [-t SEC]
│   ├── reload   (rl)    -n NAME [-s SIGNAL]
│   ├── kill              -n NAME [-s SIGNAL]
│   ├── enable            -n NAME [-b] [-W]
│   ├── disable           -n NAME [-W]
│   ├── status            [-n NAME] [-j] [-q]
│   ├── list     (ls)    [-a] [-f] [-r] [-s FIELD] [-j] [-m]
│   ├── inspect           -n NAME [-r] [-d] [-p] [-j]
│   ├── log               [-n NAME] [-f] [-l NUM] [-L LEVEL] [-g PAT] [-S TIME]
│   ├── cat               -n NAME
│   ├── edit              -n NAME [-e EDITOR]
│   ├── diff              -n NAME [-r]
│   ├── validate          [-n NAME] [-a] [-s]
│   ├── create            -n NAME -e EXEC [-r DEPS] [-R POLICY] [-s SOCK] [-d DESC] [-E ENV] [-w DIR]
│   ├── rm                -n NAME [-f] [-p]
│   ├── monitor           -n NAME [-i MS] [-t THRESH]
│   ├── watch             [-n NAME] [-e EVENTS]
│   └── tree     (dep)    [-a] [-f] [-g]
│
├── system (sys)
│   ├── boot              [-s] [-v] [-S] [-e]
│   ├── shutdown          [-f] [-t SEC] [-r]
│   ├── reboot            [-f] [-t SEC] [-m MODE]
│   ├── poweroff          [-f] [-t SEC]
│   ├── emergency         [-r REASON]
│   ├── suspend           [-H] [-y]
│   ├── resume
│   ├── freeze            [-t SEC]
│   ├── thaw
│   ├── mount             [-s SRC] [-t TGT] [-T FSTYPE] [-o OPTS] [-r] [-R]
│   ├── umount            [-t TGT] [-r] [-l] [-f] [-d]
│   ├── sync              [-f FSS] [-d]
│   ├── hostname          [-s NAME] [-S] [-l] [-t] [-j]
│   ├── uptime            [-s] [-S] [-j]
│   ├── kernel
│   │   ├── list     (ls)
│   │   ├── log           [-f] [-l NUM] [-L LEVEL]
│   │   ├── parameters (parm) [-g PAT]
│   │   └── module        [-n NAME] [-l] [-u]
│   ├── env               [-s SET] [-g GET] [-u UNSET] [-l] [-j]
│   ├── resource          [-t TYPE] [-i INT] [-f FMT] [-j]
│   ├── cgroup            [-l] [-c NAME] [-d NAME] [-a CG] [-s CG] [-p PIDS]
│   └── device            [-l] [-a DEV] [-d DEV] [-i DEV]
│
├── config (cfg)
│   ├── show              [-f FMT] [-F FILE] [-s SECTION]
│   ├── get               -k KEY [-d DEFAULT]
│   ├── set               -k KEY -v VALUE [-F FILE]
│   ├── edit              [-F FILE] [-e EDITOR] [-V]
│   ├── diff              [-F FILE] [-t TARGET] [-c NUM]
│   ├── validate          [-F FILE] [-s SCHEMA] [-S]
│   ├── import            -f FILE [-F FMT] [-m] [-o]
│   ├── export            [-f FMT] [-o FILE] [-a] [-s SECTIONS]
│   ├── backup            [-o PATH] [-c] [-l]
│   ├── restore           -f FILE [-F] [-v]
│   ├── schema            [-g] [-v] [-f FMT]
│   └── migrate           [-f FROM] [-t TO] [-d]
│
├── log (logs)
│   ├── view              [-s SVC] [-l LVL] [-g PAT] [-r] [-n NUM] [-S TIME] [-U TIME]
│   ├── tail              [-n NUM] [-s SVC] [-f] [-S SEC]
│   ├── head              [-n NUM] [-s SVC]
│   ├── grep              -p PAT [-s SVC] [-l LVL] [-c NUM] [-C]
│   ├── clear             [-s SVC] [-b TIME] [-y]
│   ├── rotate            [-s MB] [-c] [-a DIR] [-k NUM]
│   ├── archive           [-f FMT] [-o FILE] [-S TIME] [-U TIME] [-c]
│   ├── export            [-f FMT] [-o FILE] [-s SVC] [-l LVL]
│   ├── follow            [-s SVC] [-l LVL] [-g PAT] [-S SEC]
│   ├── errors            [-s SVC] [-c NUM] [-g] [-t NUM]
│   ├── warnings          [-s SVC] [-c NUM] [-g] [-t NUM]
│   ├── stats             [-s SVC] [-p PERIOD] [-f FMT]
│   └── summary           [-S TIME] [-U TIME] [-f FMT] [-t NUM]
│
├── socket (sock)
│   ├── list     (ls)     [-t TYPE] [-s STATE] [-S SVC] [-j]
│   ├── status            [-p PATH] [-v]
│   ├── create            -p PATH [-t TYPE] [-b NUM] [-s SVC]
│   ├── destroy           -p PATH [-f]
│   ├── monitor           -p PATH [-i MS] [-e EVENTS]
│   ├── trace             -p PATH [-d SEC] [-f EXPR]
│   ├── activate          -p PATH [-s SVC] [-o]
│   └── query             -p PATH [-d DATA] [-t SEC] [-n]
│
├── daemon (dmon)
│   ├── start             -n NAME [-w] [-t SEC]
│   ├── stop              -n NAME [-f] [-t SEC]
│   ├── restart           -n NAME [-f] [-t SEC]
│   ├── status            [-n NAME] [-a] [-j]
│   ├── list     (ls)     [-a] [-r] [-f] [-j]
│   ├── log               -n NAME [-f] [-l NUM]
│   ├── install           -n NAME [-f SRC] [-e] [-s]
│   ├── uninstall         -n NAME [-s] [-p]
│   ├── update            -n NAME [-f SRC] [-F]
│   ├── rollback          -n NAME [-r REV]
│   ├── pin               -n NAME [-v VER]
│   └── trust             -n NAME [-k KEY] [-v]
│
├── snapshot (snap)
│   ├── create            -n NAME [-d DESC] [-s SVCS] [-l]
│   ├── list     (ls)     [-a] [-r] [-j]
│   ├── restore           -n NAME [-f] [-d]
│   ├── delete            -n NAME [-f]
│   ├── diff              -f FROM -t TO [-s SVCS]
│   ├── export            -n NAME -o FILE [-f FMT]
│   └── import            -f FILE [-n NAME] [-F]
│
├── security (sec)
│   ├── audit             [-a] [-s] [-c] [-n]
│   ├── scan              [-t TARGETS] [-s SEVERITY] [-f]
│   ├── policy            [-s] [-S POLICY] [-v]
│   ├── cap               [-l] [-a CAP] [-d CAP] [-p PID]
│   ├── seccomp           [-l] [-a FILTER] [-d FILTER] [-p PID]
│   ├── sandbox           [-c NAME] [-d NAME] [-l] [-i NAME]
│   └── trust             [-k] [-a KEY] [-r KEY] [-v]
│
├── query (qry)
│   ├── service           [-n NAME] [-f FMT] [-F FIELDS] [-j]
│   ├── boot              [-p PHASE] [-t] [-e] [-f FMT] [-j]
│   ├── system            [-a] [-f FMT] [-F FIELDS] [-j]
│   ├── dependency        [-n NAME] [-r] [-f FMT] [-g]
│   ├── history           [-l NUM] [-f FMT] [-e EVENTS] [-j]
│   ├── resource          [-t TYPE] [-i INT] [-f FMT] [-j]
│   └── health            [-a] [-s SVC] [-f FMT] [-j]
│
├── debug (dbg)
│   ├── trace             -p PID [-s SIGNAL] [-o FILE]
│   ├── strace            -p PID [-f FILTER] [-o FILE] [-t SEC]
│   ├── dump              [-a] [-s] [-f FMT] [-o FILE]
│   ├── core              -p PID [-o FILE] [-l NUM]
│   ├── profile           -p PID [-d SEC] [-f FREQ] [-o FILE]
│   ├── stress            [-c] [-m] [-i] [-d SEC]
│   └── test              [-m MODULE] [-a] [-v]
│
└── self (me)
    ├── status            [-v]
    ├── update            [-f] [-c] [-C CHANNEL]
    ├── version           [-j] [-s]
    ├── completions       [-s SHELL] [-o FILE]
    └── config            [-s] [-S SET] [-g GET]
```

### Aliases Summary

| Command Group | Alias | Subcommand | Alias |
|---------------|-------|------------|-------|
| `service` | `svc` | `start` | `up` |
| | | `stop` | `dn` |
| | | `restart` | `rs` |
| | | `reload` | `rl` |
| | | `list` | `ls` |
| | | `tree` | `dep` |
| `system` | `sys` | | |
| `config` | `cfg` | | |
| `log` | `logs` | | |
| `socket` | `sock` | `list` | `ls` |
| `daemon` | `dmon` | `list` | `ls` |
| `snapshot` | `snap` | `list` | `ls` |
| `security` | `sec` | | |
| `query` | `qry` | | |
| `debug` | `dbg` | | |
| `self` | `me` | | |

---

</details>

<details>
<summary>Boot</summary>

When Cesar runs as PID 1:

```
1. Mount virtual filesystems (/proc, /sys, /dev)
2. Setup signal handlers:
   - SIGCHLD  -> sets atomic flag (deferred zombie reaping in main loop)
   - SIGTERM  -> sets atomic flag (deferred graceful shutdown in main loop)
   - SIGINT   -> same as SIGTERM
    - SIGHUP   -> sets atomic flag (deferred config reload in main loop)
   - SIGPIPE  -> ignored (prevents broken pipe crashes)
3. Show Plymouth splash
4. Load all .ini configs from /system/lib/cesar/services/ and /etc/cesar/services/
   (user configs in /etc/cesar/services/ override system configs with the same Name)
5. Build dependency graph (DAG) — detect cycles
6. Activate sockets for services with Socket= configs
7. Compute boot order (topological sort -> levels)
8. For each level (in parallel):
   a. Check dependency satisfaction (skip if deps not met)
   b. Mark service as Starting
   c. Fork/exec the service binary (with Environment= and WorkingDirectory= if set)
   d. Mark as Running (or Failed)
   e. Log every state transition
9. Log boot complete
10. On failure: print error tree to stderr with [CTX] and [FIX]
11. Drop to main loop:
    - Poll for zombie children (SIGCHLD deferred via atomic flag)
    - On crash: auto-restart if Restart=always (with 5-attempt limit per boot)
    - On SIGTERM: graceful shutdown (SIGTERM -> 5s grace -> SIGKILL process groups)
```

**CLI boot** (`csr` with no arguments) runs the same sequence but outputs to stdout instead of Plymouth.

---

</details>

<details>
<summary>Resolution</summary>

The DAG engine performs topological sorting with cycle detection:

```
udevd (no deps)          -> Level 1
  udev-trigger (-> udevd) -> Level 2
  tmpfiles (-> udevd)     -> Level 2
  hwclock (no deps)      -> Level 1
    elogind (-> udev-trigger) -> Level 3
      seatd (-> elogind)  -> Level 4
        pipewire (-> seatd) -> Level 5
          wireplumber (-> pipewire) -> Level 6
      iwd (-> udev-trigger) -> Level 3
        dhcpcd (-> iwd)   -> Level 4
    greetd (-> seatd, hw-detect, iwd, dhcpcd, wireplumber) -> Level 7
```

Services at the same level boot in parallel. Services at a higher level wait until all dependencies at lower levels are Running.

---

</details>

<details>
<summary>State</summary>

Each service follows this State:

```
Stopped -> Starting -> Running
                    \-> Failed
Running -> Stopping -> Stopped
Running -> Reloading -> Running
```

**Guards:**

- `is_ready()` — only allows operations when service is in a terminal state (Running, Failed, Stopped)
- `all_deps_satisfied()` — checks all `Requires` services are Running
- `mark_stopping()` / `mark_reloading()` — prevents concurrent operations

**Transitional states** (Starting, Reloading, Stopping) block new operations until the transition completes.

**Auto-restart watchdog:**

When a service exits unexpectedly (exit code != 0 or killed by signal) and has `Restart = always` (or `Restart = on-failure`):

1. Cesar detects the exit via SIGCHLD
2. Logs the failure with exit code or signal number
3. Spawns a delayed restart (1 second delay)
4. Tracks restart count per service (max 5 restarts per boot)
5. If the restart limit is exceeded, the service stays stopped and is not restarted again

Services with `Restart = never` (the default) are never auto-restarted.

---

</details>

<details>
<summary>Logging</summary>

All log output goes to `/var/log/cesar.md` in Markdown format.

| Type | Level | Example |
|------|-------|---------|
| INFO | Normal | `> [03:15:42] INFO: Spawned PID 123` |
| WARNING | Non-critical | `> [03:15:43] WARNING: Process exited unexpectedly` |
| CRITICAL | Error | `> [03:15:44] CRITICAL: Exec path not found!` |
| EVENT | State transition | `> [03:15:42] EVENT: Service started (PID 123)` |
| BOOT | System | `> [03:15:40] Boot initiated` |

Only stderr breaks silence during boot (error trees, critical messages). Everything else goes to the log file.

---

</details>

<details>
<summary>Sockets</summary>

Services can declare sockets in their `.ini` config:

```ini
Name = my-daemon
Exec = /system/bin/my-daemon
Socket = unix:/run/my-daemon.sock
```

During boot, Cesar:

1. Parses the socket spec (`unix:` -> UnixStream, `tcp:` -> TCP)
2. Creates the socket (bind + listen)
3. Starts the service
4. The service inherits the pre-created socket via `fork`/`exec`

**Supported socket types:**

| Spec | Type | Example |
|------|------|---------|
| `unix:/path/to.sock` | Unix Stream | `unix:/run/cesar.sock` |
| `unix:/path/to.sock` (no `.sock`) | Unix Datagram | `unix:/run/notify` |
| `tcp:host:port` | TCP | `tcp:0.0.0.0:8080` |
| `/path/to.sock` (default) | Unix Stream | `/run/default.sock` |

---

</details>

<details>
<summary>Snapshots</summary>

Snapshots capture the full system state for rollback:

```sh
csr snapshot create --name before-update
csr snapshot list
csr snapshot diff --from before-update --to latest
csr snapshot restore --name before-update
csr snapshot export --name before-update --output /tmp/snapshot.tar
csr snapshot import --file /tmp/snapshot.tar
```

**Snapshot contents:**

- All service configs from `/system/lib/cesar/services/` and `/etc/cesar/services/`
- Current DAG state (service states, PIDs)
- Metadata (name, description, timestamp)

Storage: `/var/lib/cesar/snapshots/`

---

</details>

<details>
<summary>Models</summary>

Cesar runs as PID 1 (root) and enforces security through:

1. **Binary permissions** — checks executability (mode & 0o111)
2. **Config ownership** — validates service config file ownership
3. **Process groups** — each service gets its own process group (via `setpgid`)
4. **Capability tracking** — reads `/proc/PID/status` for CapBnd
5. **Seccomp awareness** — reads `/proc/self/status` for Seccomp filters
6. **Network listener detection** — parses `/proc/net/tcp` for open ports
7. **Audit reporting** — `csr security audit` checks all of the above

**Process isolation:**

```
spawn_service_env():
  fork()
  setsid()            -> new session (detach from parent TTY)
  setpgid(0, 0)      -> own process group
  umask(022)
  redirect stdin/stdout/stderr -> /dev/null
  if WorkingDirectory= set:
    chdir(working_directory)
  if Environment= set:
    putenv(K=V) for each pair  (CStrings kept alive for pointer safety)
  execvp(binary, [binary, arg1, arg2, ...])  (args split from command string)
```

**Shutdown kills entire process group:**

```
kill_service_group(pid, signal):
  pgid = getpgid(pid)
  kill(-pgid, signal)  -> kills all children
```

---

</details>

<details>
<summary>Filesystem</summary>

```
/sbin/init                    -> hardlink to /system/bin/csr
/system/bin/csr                -> main binary
/system/bin/csl               -> log viewer
/system/lib/cesar/services/   -> system service configs (.ini)
/etc/cesar/services/          -> user service configs (.ini)
/etc/cesar/enabled/           -> enabled service symlinks
/var/log/cesar.md             -> markdown log file
/var/lib/cesar/snapshots/     -> snapshot storage
```

---

</details>

<details>
<summary>Building</summary>

## Building

| Profile | Command | Flags | Use case |
| ------- | ------- | ----- | -------- |
| Debug | `cargo build` | — | Development iteration, fast compile |
| Release | `cargo build --release` | `opt-level = "s"`, `lto = true`, `strip = true` | Production binary, minimised size |
| Check | `cargo check` | — | Compile-only verification, no artifacts |

```shell
# Compile-only verification (fastest)
cargo check

# Debug build
cargo build

# Release build (optimised for size)
cargo build --release
```

**Release profile:**

```toml
[profile.release]
opt-level = "s"      # Optimize for size
lto = true           # Link-time optimization
codegen-units = 1    # Single codegen unit for maximum optimization
panic = "abort"      # No unwinding
strip = true         # Strip debug symbols
```

**Output binaries:**

```
target/debug/csr       -> ~15MB (debug)
target/release/csr     -> ~2MB (release, stripped)
```

## Installation

All build systems auto-detect `x86_64`/`aarch64` and select the correct musl target. Cross-compilation files are in `env.mk`, `toolchain.cmake`, and `cross.txt` (generated via `gen-cross.sh`).

### Cargo (direct)

```shell
cargo build --release
# Binaries: target/release/csr, target/release/csl
# Install:
install -Dm755 target/release/csr /system/bin/csr
install -Dm755 target/release/csl /system/bin/csl
```

### Make

```shell
make build                    # auto-detects arch, builds for host
make install                  # installs to /system/bin/csr + csl
make install DESTDIR=/mnt     # staged install
```

### Meson

```shell
meson setup builddir --cross-file cross.txt --prefix=/system
meson compile -C builddir
meson install -C builddir
```

### Ninja

```shell
ninja -f build.ninja                       # build
ninja -f build.ninja install DESTDIR=/mnt  # staged install
```

### CMake

```shell
cmake -B build -DCMAKE_TOOLCHAIN_FILE=toolchain.cmake -DCMAKE_INSTALL_PREFIX=/system
cmake --build build
cmake --install build
```

### MCX (package manager)

```shell
mcx -i cesar
```

### System link (init)

```sh
# As part of the Cudane installer:
ln /system/bin/csr /sbin/init
```

**Integration with MCX:**

Cesar is the default init system for Cudane Linux. The [**`[MCX]`**](https://github.com/Mapuse/MCX) package manager handles:

- Installing and updating the `cesar` package
- Managing service config files via `/etc/cesar/services/`
- Coordinating with Cesar for service lifecycle events
- Package-level cgroup resource limits (`mcx --cgroup enforce`)

**Directories:**

```
/system/lib/cesar/services/    # System service configs (read-only)
/etc/cesar/services/           # User service configs (writable)
/etc/cesar/enabled/            # Enabled service symlinks
/var/log/cesar.md              # Markdown log file
/var/lib/cesar/snapshots/      # Snapshot storage
```

## Testing

```shell
# Run all tests (unit + integration)
cargo test

# Run with stdout/stderr visible
cargo test -- --nocapture

# Run a specific test by name
cargo test -- test_name

# Run with all features and release mode
cargo test --release --all-features
```

**Self-test coverage:**

| Test | What it checks |
|------|---------------|
| Config parsing | `.ini` files parse without errors |
| DAG engine | Topological sort produces correct order, cycle detection works |
| Service directory | `/system/lib/cesar/services/` and `/etc/cesar/services/` exist |
| Logger | Write + read roundtrip to `/var/log/cesar.md` |

## Linting

```shell
# Clippy (lint checks)
cargo clippy -- -D warnings

# Format check
cargo fmt --check

# Format in place
cargo fmt
```

## Auditing

```shell
# Check for security advisories in dependencies
cargo audit
```

## Debugging

```shell
# Build with debug assertions enabled in release
cargo build --profile release-debug  # requires Cargo.toml profile

# Run with RUST_LOG for tracing
RUST_LOG=debug csr

# Run with backtrace on panic
RUST_BACKTRACE=1 csr

# Run under strace for syscall tracing
strace -f -o /tmp/csr.strace ./target/release/csr

# Memory profiling with valgrind
valgrind --tool=massif ./target/release/csr
ms_print massif.out.* | less
```

## Profiling

```shell
# perf profiling (Linux)
perf record --call-graph dwarf ./target/release/csr
perf report

# Generate flamegraph
perf script | inferno-collapse-perf > stacks.folded
inferno-flamegraph stacks.folded > flamegraph.svg

# CPU sampling with perf stat
perf stat -e cycles,instructions,cache-misses,faults ./target/release/csr
```

## Continuous integration

```yaml
# Expected CI pipeline (GitHub Actions)
steps:
  - name: Checkout
    run: git checkout ${{ github.ref }}

  - name: Build
    run: cargo build --release

  - name: Test
    run: cargo test --release

  - name: Lint
    run: cargo clippy -- -D warnings

  - name: Format
    run: cargo fmt --check

  - name: Audit
    run: cargo audit
```

---

</details>

<details>
<summary>Python</summary>

The Python is a fully out-of-process plugin, theme, and TUI engine. Python runs as a separate process — Cesar never embeds an interpreter. Communication is done via:
- **CLI aliases** — on-demand execution via `csr plugin run <alias>`
- **UNIX domain socket events** — fire-and-forget JSON messages at `/run/cesar/event.sock`

### Architecture

```
┌──────────────────────────────────────────────────────────┐
│  ┌───────────────┐  ┌───────────────┐   ┌──────────────┐ │
│  │ PluginManager │  │  EventBus     │   │ CLI dispatch │ │
│  │  (p.desc)     │  │  (event.sock) │   │ plugin/      │ │
│  │  (t.desc)     │  └──────┬────────┘   │ theme/tui    │ │
│  └──────┬────────┘         │            └─────┬────────┘ │
└─────────┼──────────────────┼──────────────────┼──────────┘
          │  spawn + timeout │    emit JSON     │
          ▼                  ▼                  ▼
┌─────────────────────────────────────────────────────┐
│ Python Process(es)                                  │
│  Single-file plugins, themes, and TUIs              │
│  Each can have its own embedded venv                │
│  No restrictions — any library, any tool            │
└─────────────────────────────────────────────────────┘
```

### Configuration Files

#### `/etc/cesar/p.desc` — Plugin descriptors

```toml
[plugin.1]
name = "telegram-notifier"
path = "/etc/cesar/plugins/telegram.py"
aliases = { notify = "telegram.py --notify --channel alerts", alert = "telegram.py --alert" }

[plugin.2]
name = "web-dashboard"
path = "/etc/cesar/plugins/dashboard.py"
aliases = { dashboard = "dashboard.py --serve --port 8080", status = "dashboard.py --status" }
```

Each plugin gets a numbered section `[plugin.N]`. The `name` and `path` fields are required. The `aliases` table maps any alias string to any command string — no limits on characters, flags, or structure. The only restriction is that each alias must be unique across all plugins.

#### `/etc/cesar/t.desc` — Theme and TUI descriptors

```toml
[theme.1]
name = "catppuccin"
path = "/etc/cesar/themes/catppuccin.py"

[theme.2]
name = "nord"
path = "/etc/cesar/themes/nord.py"

[tui.1]
name = "htop-like"
path = "/etc/cesar/tuis/htop.py"

[tui.2]
name = "dashboard"
path = "/etc/cesar/tuis/dashboard.py"
```

Themes and TUIs share the same `t.desc` file but are in separate sections. `[theme.N]` for themes and `[tui.N]` for TUIs. Both require `name` and `path`.

### CLI Commands

#### Plugin commands

| Command | Description |
|---------|-------------|
| `csr plugin list` | List all installed plugins with their aliases |
| `csr plugin info <name>` | Show plugin details and file size |
| `csr plugin run <alias> [-- <args>...]` | Execute a plugin by alias with optional extra args |
| `csr plugin install <path> [-n <name>] [-a <alias>] [-A k=v...]` | Install a plugin file and register it |
| `csr plugin remove <name>` | Uninstall a plugin |

#### Theme commands

| Command | Description |
|---------|-------------|
| `csr theme list` | List all installed themes |
| `csr theme info <name>` | Show theme details |
| `csr theme apply <name>` | Execute a theme (runs its Python file) |
| `csr theme install <path> [-n <name>]` | Install a theme file and register it |
| `csr theme remove <name>` | Uninstall a theme |

#### TUI commands

| Command | Description |
|---------|-------------|
| `csr tui list` | List all installed TUIs |
| `csr tui info <name>` | Show TUI details |
| `csr tui apply <name>` | Launch a TUI (runs its Python file) |
| `csr tui install <path> [-n <name>]` | Install a TUI file and register it |
| `csr tui remove <name>` | Uninstall a TUI |

### Plugin Execution Model

1. **Alias lookup** — `csr plugin run notify` matches the alias in `p.desc`
2. **VENV detection** — Cesar checks for `<script>.py.venv/` or `<script-dir>/venv/` and uses `bin/python3` from the venv if found; falls back to system `python3` otherwise
3. **Spawn** — The command string from the alias + any extra args are passed to `python3`
4. **Timeout** — Default 30s timeout. If the plugin exceeds it, Cesar sends `SIGKILL`
5. **Output** — stdout is returned on success, stderr on failure

All execution is non-blocking fire-and-forget at the Cesar level. Plugins run as independent processes and cannot crash the init system.

### Event Bus

Cesar emits JSON events to `/run/cesar/event.sock` (UNIX datagram) during boot and service lifecycle. Plugins can listen on this socket to react to system events:

| Event | Payload | When |
|-------|---------|------|
| `boot` | `{"state": "starting"}` | Boot begins |
| `boot` | `{"total_services": N, "failed_services": N}` | Boot complete |
| `service` | `{"name": "...", "state": "started", "pid": N}` | Service started |
| `service` | `{"name": "...", "state": "failed", "pid": 0}` | Service failed |
| `service` | `{"name": "...", "state": "restarted", "pid": N}` | Service auto-restarted |
| `service` | `{"name": "...", "state": "restart-failed", "pid": 0}` | Auto-restart failed |
| `shutdown` | `null` | Graceful shutdown initiated |

A Python plugin listening for events:

```python
import json, socket, os

SOCKET_PATH = "/run/cesar/event.sock"

if os.path.exists(SOCKET_PATH):
    os.remove(SOCKET_PATH)

sock = socket.socket(socket.AF_UNIX, socket.SOCK_DGRAM)
sock.bind(SOCKET_PATH)
sock.settimeout(1.0)

while True:
    try:
        data, _ = sock.recvfrom(4096)
        event = json.loads(data)
        if event["event"] == "service" and event["data"]["state"] == "failed":
            print(f"ALERT: Service {event['data']['name']} failed!")
    except socket.timeout:
        continue
```

### VENV Support

Each plugin/theme/TUI is a single Python file that can manage its own virtual environment. Cesar auto-detects venvs in two locations (checked in order):

1. **Adjacent** — `<script-path>.py.venv/` (e.g., `telegram.py.venv/`)
2. **Shared** — `<script-dir>/venv/`

If found, Cesar uses `<venv>/bin/python3` as the interpreter. The Python file itself is responsible for creating its venv on first run, installing dependencies, etc.

### No Restrictions

- **No specific structure** — A plugin can be 10 lines or 10,000 lines
- **No naming conventions** — Names can contain any characters
- **No alias limits** — Any alias string, any flag combination, any command string
- **No library restrictions** — Import anything from the Python ecosystem
- **No file size limits** — A single file can contain an entire application
- **No hooks required** — The file is just a Python script; what it does is entirely up to the author

### Example Plugin

```python
#!/usr/bin/env python3
"""telegram-notifier: Send alerts to Telegram when services fail."""

import json, os, sys, requests

VENV_DIR = os.path.join(os.path.dirname(__file__), f"{os.path.basename(__file__)}.venv")

def ensure_venv():
    if not os.path.exists(VENV_DIR):
        import subprocess
        subprocess.run([sys.executable, "-m", "venv", VENV_DIR], check=True)
        pip = os.path.join(VENV_DIR, "bin", "pip")
        subprocess.run([pip, "install", "requests"], check=True)

def main():
    ensure_venv()
    # Use the venv's Python for the actual logic
    msg = " ".join(sys.argv[1:]) or "Service alert from Cesar"
    token = os.environ.get("TELEGRAM_TOKEN", "")
    chat_id = os.environ.get("TELEGRAM_CHAT", "")
    if token and chat_id:
        requests.post(f"https://api.telegram.org/bot{token}/sendMessage",
                      json={"chat_id": chat_id, "text": msg})

if __name__ == "__main__":
    main()
```

---

</details>

<details>
<summary>Configuration</summary>

Cesar reads its main configuration from `/etc/cesar/cesar.ini`. All fields have defaults, so the file is optional.

### `[Cesar]` section

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `level` | string | `info` | Log level: `debug`, `info`, `warning`, `error` |
| `log_file` | string | `/var/log/cesar.md` | Path to the markdown log file |

### `[Plugin]` section

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `enabled` | bool | `true` | Enable/disable the entire plugin subsystem |
| `timeout` | integer | `30` | Maximum execution time per plugin (seconds) |
| `event_socket` | string | `/run/cesar/event.sock` | UNIX datagram socket path for event emission |

### `[TUI]` section

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `enabled` | bool | `true` | Enable/disable TUI features |
| `theme` | string | `default` | Theme to load at startup |
| `default_tui` | string | `default` | Default TUI to launch |
| `interval` | integer | `1000` | UI refresh rate (milliseconds) |
| `scrollback` | integer | `5000` | Scrollback buffer size (lines) |
| `enable_mouse` | bool | `true` | Enable mouse support |
| `enable_ansi` | bool | `true` | Enable ANSI escape code support |
| `color_depth` | string | `truecolor` | Color depth: `truecolor`, `256`, `16`, `8` |
| `status_bar` | bool | `true` | Show status bar |
| `keybindings` | string | `default` | Keybinding profile: `default`, `vim`, `emacs`, `none` |
| `border_style` | string | `rounded` | Border style: `rounded`, `sharp`, `double`, `none` |
| `show_help` | bool | `true` | Show help bar |
| `show_status` | bool | `true` | Show status indicators |
| `show_uptime` | bool | `true` | Show system uptime |
| `show_services` | bool | `true` | Show service list |
| `show_resources` | bool | `true` | Show resource usage |
| `show_logs` | bool | `true` | Show log panel |
| `compact` | bool | `false` | Compact layout (hide labels, minimize spacing) |
| `vim` | bool | `false` | Vim-style navigation keys |
| `notifications` | bool | `true` | Enable desktop-style notifications |
| `notification_timeout` | integer | `5000` | Notification dismiss timeout (milliseconds) |
| `animation` | bool | `true` | Enable UI animations |
| `animation` | string | `normal` | Animation speed: `slow`, `normal`, `fast`, `instant` |
| `font_size` | integer | `12` | Terminal font size (points) |
| `line_height` | float | `1.0` | Line height multiplier |
| `letter_spacing` | float | `0.0` | Letter spacing (points) |
| `padding` | integer | `2` | Padding around content (cells) |
| `margin` | integer | `1` | Margin around UI (cells) |
| `scroll_offset` | integer | `0` | Lines to keep visible above/below cursor |
| `wrap` | bool | `true` | Word wrap in text panels |
| `tab_width` | integer | `4` | Tab stop width |
| `cursor_style` | string | `block` | Cursor shape: `block`, `underline`, `bar`, `hidden` |
| `cursor_blink` | bool | `true` | Blinking cursor |
| `selection_clipboard` | bool | `true` | Copy selection to clipboard |
| `link_underline` | string | `hover` | Link underline mode: `hover`, `always`, `never` |
| `search_case_sensitive` | bool | `false` | Default case sensitivity for search |
| `search_regex` | bool | `false` | Enable regex search by default |
| `search_highlight_color` | string | `yellow` | Color for search matches |
| `bell` | string | `visual` | Bell style: `visual`, `audible`, `none` |

### Example `/etc/cesar/cesar.ini`

```ini
[Cesar]
level = debug

[Plugin]
enabled = true
timeout = 60

[TUI]
enabled = true
theme = catppuccin
color_depth = 256
compact = true
vim = true
scrollback = 10000
interval = 500
animation = fast
```

---

</details>

<details>
<summary>Structure</summary>

```
Cesar/
├── Cargo.toml              # Package (v0.0.7, edition 2024)
├── LICENSE                 # MIT License
├── .gitignore              # Rust/IDE/OS ignores
├── .gitattributes          # Codeberg linguist hints
├── services/               # Example service configs
│   ├── udevd.ini
│   ├── elogind.ini
│   ├── greetd.ini
│   └── ...
└── src/
    ├── main.rs             # PID 1 entry, signal handlers, Boot
    ├── lib.rs              # Module declarations
    ├── cli.rs              # Clap CLI (2,162 lines)
    ├── commands.rs         # All command handlers (1,993 lines)
    ├── config.rs           # INI config parser
    ├── dag.rs              # DAG dependency engine
    ├── logger.rs           # Markdown logger
    ├── process.rs          # fork/exec, zombie reaping, process groups
    ├── service.rs          # Core types (ServiceState, Service, ServiceConfig)
    ├── socket.rs           # Sockets
    ├── visual.rs           # Error trees and status dashboard
    └── bin/
        └── csl.rs          # Log viewer binary
```

---

</details>

<details>
<summary>Dependencies</summary>

| Crate | Version | Purpose |
|-------|---------|---------|
| `tokio` | 1 (full) | Async runtime for parallel boot |
| `libc` | 0.2 | System calls (fork, exec, kill, mount, reboot) |
| `nix` | 0.29 | Safe Rust wrappers for signals and process management |
| `chrono` | 0.4 | Timestamps in log entries |
| `clap` | 4 (derive) | CLI argument parsing with derive macros |
| `hostname` | 0.4 | System hostname detection |

---

</details>

<details>
<summary>Contributing</summary>

Cesar on [[**`[GitHub]`**]](https://github.com/Mapuse). Issues and pull requests are welcome.

```sh
git clone https://github.com/Mapuse/Cesar.git
cd Cesar
cargo check
cargo build --release
cargo test -- --nocapture
```

Follow existing code style. No comments unless requested. All error paths must provide real diagnostics with `[CTX]` and `[FIX]` annotations.

---

</details>

## Credits

**`[Cesar]`** is part of the **`[Cudane]`** ecosystem.

- **`[Cudane]`** — The Distribution.
- **`[MCX]`** — Package Manager.

## License

**MIT License** ─ See [**`[LICENSE]`**](https://github.com/Mapuse/.github/blob/profile/LICENSE) for More Details.
