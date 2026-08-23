use clap::{Parser, Subcommand, Args};

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Parser)]
#[command(
    name = "csr",
    version = VERSION,
    about = "Cesar Init System — Sovereign PID 1 for Cudane",
    long_about = "Cesar is the init system for the Cudane distribution.\nIt manages services via an async DAG engine, handles socket activation,\nand provides a unified Markdown logging system.",
    subcommand_help_heading = "Command Groups",
    subcommand_required = false,
    args_conflicts_with_subcommands = true,
    disable_help_subcommand = true
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<TopCommand>,
}

#[derive(Subcommand)]
pub enum TopCommand {

    #[command(alias("svc"), subcommand)]
    Service(ServiceCommand),


    #[command(alias("sys"), subcommand)]
    System(SystemCommand),


    #[command(alias("cfg"), subcommand)]
    Config(ConfigCommand),


    #[command(alias("logs"), subcommand)]
    Log(LogCommand),


    #[command(alias("sock"), subcommand)]
    Socket(SocketCommand),


    #[command(alias("dmon"), subcommand)]
    Daemon(DaemonCommand),


    #[command(alias("snap"), subcommand)]
    Snapshot(SnapshotCommand),


    #[command(alias("sec"), subcommand)]
    Security(SecurityCommand),


    #[command(alias("qry"), subcommand)]
    Query(QueryCommand),


    #[command(alias("dbg"), subcommand)]
    Debug(DebugCommand),


    #[command(alias("me"), subcommand)]
    Self_(SelfCommand),


    #[command(alias("plug"), subcommand)]
    Plugin(PluginCommand),


    #[command(alias("thm"), subcommand)]
    Theme(ThemeCommand),


    #[command(subcommand)]
    Tui(TuiCommand),
}


#[derive(Subcommand)]
pub enum ServiceCommand {

    #[command(alias("up"))]
    Start(ServiceNameArgs),


    #[command(alias("dn"))]
    Stop(ServiceForceArgs),


    #[command(alias("rs"))]
    Restart(ServiceForceTimeoutArgs),


    #[command(alias("rl"))]
    Reload(ServiceSignalArgs),


    Kill(ServiceSignalArgs),


    Enable(ServiceEnableArgs),


    Disable(ServiceDisableArgs),


    Status(ServiceStatusArgs),


    #[command(alias("ls"))]
    List(ServiceListArgs),


    Inspect(ServiceInspectArgs),


    Log(ServiceLogArgs),


    Cat(ServiceNameArgs),


    Edit(ServiceEditArgs),


    Diff(ServiceDiffArgs),


    Validate(ServiceValidateArgs),


    Create(ServiceCreateArgs),


    #[command(alias("conv"))]
    Convert(ServiceConvertArgs),


    Rm(ServiceRemoveArgs),


    Monitor(ServiceMonitorArgs),


    Watch(ServiceWatchArgs),


    #[command(alias("dep"))]
    Tree(ServiceTreeArgs),
}

#[derive(Args)]
pub struct ServiceNameArgs {

    #[arg(short = 'n', long = "name")]
    pub name: String,

    #[arg(short = 'f', long = "force")]
    pub force: bool,

    #[arg(short = 'w', long = "wait")]
    pub wait: bool,


    #[arg(short = 't', long = "timeout", default_value = "30")]
    pub timeout: u64,
}

pub type ServiceForceArgs = ServiceForceTimeoutArgs;

#[derive(Args)]
pub struct ServiceForceTimeoutArgs {

    #[arg(short = 'n', long = "name")]
    pub name: String,


    #[arg(short = 'f', long = "force")]
    pub force: bool,


    #[arg(short = 't', long = "timeout", default_value = "30")]
    pub timeout: u64,
}

#[derive(Args)]
pub struct ServiceSignalArgs {

    #[arg(short = 'n', long = "name")]
    pub name: String,


    #[arg(short = 's', long = "signal")]
    pub signal: Option<String>,
}

#[derive(Args)]
pub struct ServiceEnableArgs {

    #[arg(short = 'n', long = "name")]
    pub name: String,


    #[arg(short = 'b', long = "boot")]
    pub boot: bool,


    #[arg(short = 'W', long = "now")]
    pub now: bool,
}

#[derive(Args)]
pub struct ServiceDisableArgs {

    #[arg(short = 'n', long = "name")]
    pub name: String,


    #[arg(short = 'W', long = "now")]
    pub now: bool,
}

#[derive(Args)]
pub struct ServiceStatusArgs {

    #[arg(short = 'n', long = "name")]
    pub name: Option<String>,


    #[arg(short = 'j', long = "json")]
    pub json: bool,


    #[arg(short = 'q', long = "quiet")]
    pub quiet: bool,
}

#[derive(Args)]
pub struct ServiceListArgs {

    #[arg(short = 'a', long = "all")]
    pub all: bool,


    #[arg(short = 'f', long = "failed")]
    pub failed: bool,


    #[arg(short = 'r', long = "running")]
    pub running: bool,


    #[arg(short = 's', long = "sort")]
    pub sort: Option<String>,


    #[arg(short = 'j', long = "json")]
    pub json: bool,


    #[arg(short = 'm', long = "minimal")]
    pub minimal: bool,
}

#[derive(Args)]
pub struct ServiceInspectArgs {

    #[arg(short = 'n', long = "name")]
    pub name: String,


    #[arg(short = 'r', long = "raw")]
    pub raw: bool,


    #[arg(short = 'd', long = "deps")]
    pub deps: bool,


    #[arg(short = 'p', long = "pid")]
    pub pid: bool,


    #[arg(short = 'j', long = "json")]
    pub json: bool,
}

#[derive(Args)]
pub struct ServiceLogArgs {

    #[arg(short = 'n', long = "name")]
    pub name: Option<String>,


    #[arg(short = 'f', long = "follow")]
    pub follow: bool,


    #[arg(short = 'l', long = "lines", default_value = "50")]
    pub lines: usize,


    #[arg(short = 'L', long = "level")]
    pub level: Option<String>,


    #[arg(short = 'g', long = "grep")]
    pub grep: Option<String>,


    #[arg(short = 'S', long = "since")]
    pub since: Option<String>,
}

#[derive(Args)]
pub struct ServiceEditArgs {

    #[arg(short = 'n', long = "name")]
    pub name: String,


    #[arg(short = 'e', long = "editor")]
    pub editor: Option<String>,
}

#[derive(Args)]
pub struct ServiceDiffArgs {

    #[arg(short = 'n', long = "name")]
    pub name: String,


    #[arg(short = 'r', long = "running")]
    pub running: bool,
}

#[derive(Args)]
pub struct ServiceValidateArgs {

    #[arg(short = 'n', long = "name")]
    pub name: Option<String>,


    #[arg(short = 'a', long = "all")]
    pub all: bool,


    #[arg(short = 's', long = "strict")]
    pub strict: bool,
}

#[derive(Args)]
pub struct ServiceCreateArgs {

    #[arg(short = 'n', long = "name")]
    pub name: String,


    #[arg(short = 'e', long = "exec")]
    pub exec: String,


    #[arg(short = 'r', long = "requires")]
    pub requires: Option<String>,


    #[arg(short = 'R', long = "restart")]
    pub restart: Option<String>,


    #[arg(short = 's', long = "socket")]
    pub socket: Option<String>,


    #[arg(short = 'd', long = "description")]
    pub description: Option<String>,
}

#[derive(Args)]
pub struct ServiceConvertArgs {

    #[arg(short = 's', long = "source", default_value = "/etc/systemd/system")]
    pub source: String,


    #[arg(short = 'd', long = "dest", default_value = "/etc/cesar/services")]
    pub dest: String,


    #[arg(short = 'f', long = "force")]
    pub force: bool,


    /// Explicitly remove the source directory after a successful conversion.
    /// Never the default: deleting units is destructive.
    #[arg(long = "remove-source")]
    pub remove_source: bool,
}

#[derive(Args)]
pub struct ServiceRemoveArgs {

    #[arg(short = 'n', long = "name")]
    pub name: String,


    #[arg(short = 'f', long = "force")]
    pub force: bool,


    #[arg(short = 'p', long = "purge")]
    pub purge: bool,
}

#[derive(Args)]
pub struct ServiceMonitorArgs {

    #[arg(short = 'n', long = "name")]
    pub name: String,


    #[arg(short = 'i', long = "interval", default_value = "1000")]
    pub interval: u64,


    #[arg(short = 't', long = "threshold", default_value = "3")]
    pub threshold: u32,
}

#[derive(Args)]
pub struct ServiceWatchArgs {

    #[arg(short = 'n', long = "name")]
    pub name: Option<String>,


    #[arg(short = 'e', long = "events")]
    pub events: Option<String>,
}

#[derive(Args)]
pub struct ServiceTreeArgs {

    #[arg(short = 'a', long = "all")]
    pub all: bool,


    #[arg(short = 'f', long = "flat")]
    pub flat: bool,


    #[arg(short = 'g', long = "graph")]
    pub graph: bool,
}


#[derive(Subcommand)]
pub enum SystemCommand {

    Boot(SystemBootArgs),


    Shutdown(SystemShutdownArgs),


    Reboot(SystemRebootArgs),


    Poweroff(SystemPoweroffArgs),


    Emergency(SystemEmergencyArgs),


    Suspend(SystemSuspendArgs),


    Resume,


    Freeze(SystemFreezeArgs),


    Thaw,


    Mount(SystemMountArgs),


    Umount(SystemUmountArgs),


    Sync(SystemSyncArgs),


    Hostname(SystemHostnameArgs),


    Uptime(SystemUptimeArgs),


    Kernel(SystemKernelArgs),


    Env(SystemEnvArgs),


    Resource(SystemResourceArgs),


    Cgroup(SystemCgroupArgs),


    Device(SystemDeviceArgs),
}

#[derive(Args)]
pub struct SystemBootArgs {

    #[arg(short = 's', long = "splash")]
    pub splash: bool,


    #[arg(short = 'v', long = "verbose")]
    pub verbose: bool,


    #[arg(short = 'S', long = "single")]
    pub single: bool,


    #[arg(short = 'e', long = "emergency")]
    pub emergency: bool,
}

#[derive(Args)]
pub struct SystemShutdownArgs {

    #[arg(short = 'f', long = "force")]
    pub force: bool,


    #[arg(short = 't', long = "timeout", default_value = "60")]
    pub timeout: u64,


    #[arg(short = 'r', long = "reboot")]
    pub reboot: bool,
}

#[derive(Args)]
pub struct SystemRebootArgs {

    #[arg(short = 'f', long = "force")]
    pub force: bool,


    #[arg(short = 't', long = "timeout", default_value = "60")]
    pub timeout: u64,


    #[arg(short = 'm', long = "mode")]
    pub mode: Option<String>,
}

#[derive(Args)]
pub struct SystemPoweroffArgs {

    #[arg(short = 'f', long = "force")]
    pub force: bool,


    #[arg(short = 't', long = "timeout", default_value = "60")]
    pub timeout: u64,
}

#[derive(Args)]
pub struct SystemEmergencyArgs {

    #[arg(short = 'r', long = "reason")]
    pub reason: Option<String>,
}

#[derive(Args)]
pub struct SystemSuspendArgs {

    #[arg(short = 'H', long = "hibernate")]
    pub hibernate: bool,


    #[arg(short = 'y', long = "hybrid")]
    pub hybrid: bool,
}

#[derive(Args)]
pub struct SystemFreezeArgs {

    #[arg(short = 't', long = "timeout")]
    pub timeout: Option<u64>,
}

#[derive(Args)]
pub struct SystemMountArgs {

    #[arg(short = 's', long = "source")]
    pub source: Option<String>,


    #[arg(short = 't', long = "target")]
    pub target: Option<String>,


    #[arg(short = 'T', long = "type")]
    pub fs_type: Option<String>,


    #[arg(short = 'o', long = "options")]
    pub options: Option<String>,


    #[arg(short = 'r', long = "recursive")]
    pub recursive: bool,


    #[arg(short = 'R', long = "remount")]
    pub remount: bool,
}

#[derive(Args)]
pub struct SystemUmountArgs {

    #[arg(short = 't', long = "target")]
    pub target: Option<String>,


    #[arg(short = 'r', long = "recursive")]
    pub recursive: bool,


    #[arg(short = 'l', long = "lazy")]
    pub lazy: bool,


    #[arg(short = 'f', long = "force")]
    pub force: bool,


    #[arg(short = 'd', long = "detach")]
    pub detach: bool,
}

#[derive(Args)]
pub struct SystemSyncArgs {

    #[arg(short = 'f', long = "file-systems")]
    pub file_systems: Option<String>,


    #[arg(short = 'd', long = "data")]
    pub data: bool,
}

#[derive(Args)]
pub struct SystemHostnameArgs {

    #[arg(short = 's', long = "set")]
    pub set: Option<String>,


    #[arg(short = 'S', long = "short")]
    pub short: bool,


    #[arg(short = 'l', long = "long")]
    pub long: bool,


    #[arg(short = 't', long = "static")]
    pub static_: bool,


    #[arg(short = 'j', long = "json")]
    pub json: bool,
}

#[derive(Args)]
pub struct SystemUptimeArgs {

    #[arg(short = 's', long = "since")]
    pub since: bool,


    #[arg(short = 'S', long = "seconds")]
    pub seconds: bool,


    #[arg(short = 'j', long = "json")]
    pub json: bool,
}

#[derive(Args)]
pub struct SystemKernelArgs {

    #[command(subcommand)]
    pub command: Option<KernelSubCommand>,
}

#[derive(Subcommand)]
pub enum KernelSubCommand {

    #[command(alias("ls"))]
    List,


    Log {

        #[arg(short = 'f', long = "follow")]
        follow: bool,


        #[arg(short = 'l', long = "lines", default_value = "100")]
        lines: usize,


        #[arg(short = 'L', long = "level")]
        level: Option<String>,
    },


    #[command(alias("parm"))]
    Parameters {

        #[arg(short = 'g', long = "grep")]
        grep: Option<String>,
    },


    Module {

        #[arg(short = 'n', long = "name")]
        name: Option<String>,


        #[arg(short = 'l', long = "load")]
        load: bool,


        #[arg(short = 'u', long = "unload")]
        unload: bool,
    },
}

#[derive(Args)]
pub struct SystemEnvArgs {

    #[arg(short = 's', long = "set")]
    pub set: Option<String>,


    #[arg(short = 'g', long = "get")]
    pub get: Option<String>,


    #[arg(short = 'u', long = "unset")]
    pub unset: Option<String>,


    #[arg(short = 'l', long = "list")]
    pub list: bool,


    #[arg(short = 'j', long = "json")]
    pub json: bool,
}

#[derive(Args)]
pub struct SystemResourceArgs {

    #[arg(short = 't', long = "type")]
    pub resource_type: Option<String>,


    #[arg(short = 'i', long = "interval")]
    pub interval: Option<u64>,


    #[arg(short = 'f', long = "format")]
    pub format: Option<String>,


    #[arg(short = 'j', long = "json")]
    pub json: bool,
}

#[derive(Args)]
pub struct SystemCgroupArgs {

    #[arg(short = 'l', long = "list")]
    pub list: bool,


    #[arg(short = 'c', long = "create")]
    pub create: Option<String>,


    #[arg(short = 'd', long = "destroy")]
    pub destroy: Option<String>,


    #[arg(short = 'a', long = "attach")]
    pub attach: Option<String>,


    #[arg(short = 's', long = "stats")]
    pub stats: Option<String>,


    #[arg(short = 'p', long = "pids")]
    pub pids: Option<String>,
}

#[derive(Args)]
pub struct SystemDeviceArgs {

    #[arg(short = 'l', long = "list")]
    pub list: bool,


    #[arg(short = 'a', long = "attach")]
    pub attach: Option<String>,


    #[arg(short = 'd', long = "detach")]
    pub detach: Option<String>,


    #[arg(short = 'i', long = "info")]
    pub info: Option<String>,
}


#[derive(Subcommand)]
pub enum ConfigCommand {

    Show(ConfigShowArgs),


    Get(ConfigGetArgs),


    Set(ConfigSetArgs),


    Edit(ConfigEditArgs),


    Diff(ConfigDiffArgs),


    Validate(ConfigValidateArgs),


    Import(ConfigImportArgs),


    Export(ConfigExportArgs),


    Backup(ConfigBackupArgs),


    Restore(ConfigRestoreArgs),


    Schema(ConfigSchemaArgs),


    Migrate(ConfigMigrateArgs),
}

#[derive(Args)]
pub struct ConfigShowArgs {

    #[arg(short = 'f', long = "format")]
    pub format: Option<String>,


    #[arg(short = 'F', long = "file")]
    pub file: Option<String>,


    #[arg(short = 's', long = "section")]
    pub section: Option<String>,
}

#[derive(Args)]
pub struct ConfigGetArgs {

    #[arg(short = 'k', long = "key")]
    pub key: String,


    #[arg(short = 'd', long = "default")]
    pub default: Option<String>,
}

#[derive(Args)]
pub struct ConfigSetArgs {

    #[arg(short = 'k', long = "key")]
    pub key: String,


    #[arg(short = 'v', long = "value")]
    pub value: String,


    #[arg(short = 'F', long = "file")]
    pub file: Option<String>,
}

#[derive(Args)]
pub struct ConfigEditArgs {

    #[arg(short = 'F', long = "file")]
    pub file: Option<String>,


    #[arg(short = 'e', long = "editor")]
    pub editor: Option<String>,


    #[arg(short = 'V', long = "validate")]
    pub validate: bool,
}

#[derive(Args)]
pub struct ConfigDiffArgs {

    #[arg(short = 'F', long = "file")]
    pub file: Option<String>,


    #[arg(short = 't', long = "target")]
    pub target: Option<String>,


    #[arg(short = 'c', long = "context")]
    pub context: Option<usize>,
}

#[derive(Args)]
pub struct ConfigValidateArgs {

    #[arg(short = 'F', long = "file")]
    pub file: Option<String>,


    #[arg(short = 's', long = "schema")]
    pub schema: Option<String>,


    #[arg(short = 'S', long = "strict")]
    pub strict: bool,
}

#[derive(Args)]
pub struct ConfigImportArgs {

    #[arg(short = 'f', long = "file")]
    pub file: String,


    #[arg(short = 'F', long = "format")]
    pub format: Option<String>,


    #[arg(short = 'm', long = "merge")]
    pub merge: bool,


    #[arg(short = 'o', long = "overwrite")]
    pub overwrite: bool,
}

#[derive(Args)]
pub struct ConfigExportArgs {

    #[arg(short = 'f', long = "format")]
    pub format: Option<String>,


    #[arg(short = 'o', long = "output")]
    pub output: Option<String>,


    #[arg(short = 'a', long = "all")]
    pub all: bool,


    #[arg(short = 's', long = "sections")]
    pub sections: Option<String>,
}

#[derive(Args)]
pub struct ConfigBackupArgs {

    #[arg(short = 'o', long = "output")]
    pub output: Option<String>,


    #[arg(short = 'c', long = "compress")]
    pub compress: bool,


    #[arg(short = 'l', long = "include-logs")]
    pub include_logs: bool,
}

#[derive(Args)]
pub struct ConfigRestoreArgs {

    #[arg(short = 'f', long = "file")]
    pub file: String,


    #[arg(short = 'F', long = "force")]
    pub force: bool,


    #[arg(short = 'v', long = "verify")]
    pub verify: bool,
}

#[derive(Args)]
pub struct ConfigSchemaArgs {

    #[arg(short = 'g', long = "generate")]
    pub generate: bool,


    #[arg(short = 'v', long = "validate")]
    pub validate: bool,


    #[arg(short = 'f', long = "format")]
    pub format: Option<String>,
}

#[derive(Args)]
pub struct ConfigMigrateArgs {

    #[arg(short = 'f', long = "from")]
    pub from: Option<String>,


    #[arg(short = 't', long = "to")]
    pub to: Option<String>,


    #[arg(short = 'd', long = "dry-run")]
    pub dry_run: bool,
}


#[derive(Subcommand)]
pub enum LogCommand {

    View(LogViewArgs),


    Tail(LogTailArgs),


    Head(LogHeadArgs),


    Grep(LogGrepArgs),


    Clear(LogClearArgs),


    Rotate(LogRotateArgs),


    Archive(LogArchiveArgs),


    Export(LogExportArgs),


    Follow(LogFollowArgs),


    Errors(LogFilterArgs),


    Warnings(LogFilterArgs),


    Stats(LogStatsArgs),


    Summary(LogSummaryArgs),
}

#[derive(Args)]
pub struct LogViewArgs {

    #[arg(short = 's', long = "service")]
    pub service: Option<String>,


    #[arg(short = 'l', long = "level")]
    pub level: Option<String>,


    #[arg(short = 'g', long = "grep")]
    pub grep: Option<String>,


    #[arg(short = 'r', long = "reverse")]
    pub reverse: bool,


    #[arg(short = 'n', long = "lines", default_value = "100")]
    pub lines: usize,


    #[arg(short = 'S', long = "since")]
    pub since: Option<String>,


    #[arg(short = 'U', long = "until")]
    pub until: Option<String>,
}

#[derive(Args)]
pub struct LogTailArgs {

    #[arg(short = 'n', long = "lines", default_value = "50")]
    pub lines: usize,


    #[arg(short = 's', long = "service")]
    pub service: Option<String>,


    #[arg(short = 'f', long = "follow")]
    pub follow: bool,


    #[arg(short = 'S', long = "sleep", default_value = "1")]
    pub sleep: u64,
}

#[derive(Args)]
pub struct LogHeadArgs {

    #[arg(short = 'n', long = "lines", default_value = "50")]
    pub lines: usize,


    #[arg(short = 's', long = "service")]
    pub service: Option<String>,
}

#[derive(Args)]
pub struct LogGrepArgs {

    #[arg(short = 'p', long = "pattern")]
    pub pattern: String,


    #[arg(short = 's', long = "service")]
    pub service: Option<String>,


    #[arg(short = 'l', long = "level")]
    pub level: Option<String>,


    #[arg(short = 'c', long = "context")]
    pub context: Option<usize>,


    #[arg(short = 'C', long = "count")]
    pub count: bool,
}

#[derive(Args)]
pub struct LogClearArgs {

    #[arg(short = 's', long = "service")]
    pub service: Option<String>,


    #[arg(short = 'b', long = "before")]
    pub before: Option<String>,


    #[arg(short = 'y', long = "yes")]
    pub yes: bool,
}

#[derive(Args)]
pub struct LogRotateArgs {

    #[arg(short = 's', long = "max-size", default_value = "10")]
    pub max_size: usize,


    #[arg(short = 'c', long = "compress")]
    pub compress: bool,


    #[arg(short = 'a', long = "archive")]
    pub archive: Option<String>,


    #[arg(short = 'k', long = "keep", default_value = "5")]
    pub keep: usize,
}

#[derive(Args)]
pub struct LogArchiveArgs {

    #[arg(short = 'f', long = "format")]
    pub format: Option<String>,


    #[arg(short = 'o', long = "output")]
    pub output: Option<String>,


    #[arg(short = 'S', long = "since")]
    pub since: Option<String>,


    #[arg(short = 'U', long = "until")]
    pub until: Option<String>,


    #[arg(short = 'c', long = "compress")]
    pub compress: bool,
}

#[derive(Args)]
pub struct LogExportArgs {

    #[arg(short = 'f', long = "format")]
    pub format: Option<String>,


    #[arg(short = 'o', long = "output")]
    pub output: Option<String>,


    #[arg(short = 's', long = "service")]
    pub service: Option<String>,


    #[arg(short = 'l', long = "level")]
    pub level: Option<String>,
}

#[derive(Args)]
pub struct LogFollowArgs {

    #[arg(short = 's', long = "service")]
    pub service: Option<String>,


    #[arg(short = 'l', long = "level")]
    pub level: Option<String>,


    #[arg(short = 'g', long = "grep")]
    pub grep: Option<String>,


    #[arg(short = 'S', long = "sleep", default_value = "1")]
    pub sleep: u64,
}

#[derive(Args)]
pub struct LogFilterArgs {

    #[arg(short = 's', long = "service")]
    pub service: Option<String>,


    #[arg(short = 'c', long = "context")]
    pub context: Option<usize>,


    #[arg(short = 'g', long = "group")]
    pub group: bool,


    #[arg(short = 't', long = "top")]
    pub top: Option<usize>,
}

#[derive(Args)]
pub struct LogStatsArgs {

    #[arg(short = 's', long = "service")]
    pub service: Option<String>,


    #[arg(short = 'p', long = "period")]
    pub period: Option<String>,


    #[arg(short = 'f', long = "format")]
    pub format: Option<String>,
}

#[derive(Args)]
pub struct LogSummaryArgs {

    #[arg(short = 'S', long = "since")]
    pub since: Option<String>,


    #[arg(short = 'U', long = "until")]
    pub until: Option<String>,


    #[arg(short = 'f', long = "format")]
    pub format: Option<String>,


    #[arg(short = 't', long = "top")]
    pub top: Option<usize>,
}


#[derive(Subcommand)]
pub enum SocketCommand {

    #[command(alias("ls"))]
    List(SocketListArgs),


    Status(SocketStatusArgs),


    Create(SocketCreateArgs),


    Destroy(SocketDestroyArgs),


    Monitor(SocketMonitorArgs),


    Trace(SocketTraceArgs),


    Activate(SocketActivateArgs),


    Query(SocketQueryArgs),
}

#[derive(Args)]
pub struct SocketListArgs {

    #[arg(short = 't', long = "type")]
    pub socket_type: Option<String>,


    #[arg(short = 's', long = "state")]
    pub state: Option<String>,


    #[arg(short = 'S', long = "service")]
    pub service: Option<String>,


    #[arg(short = 'j', long = "json")]
    pub json: bool,
}

#[derive(Args)]
pub struct SocketStatusArgs {

    #[arg(short = 'p', long = "path")]
    pub path: Option<String>,


    #[arg(short = 'v', long = "verbose")]
    pub verbose: bool,
}

#[derive(Args)]
pub struct SocketCreateArgs {

    #[arg(short = 'p', long = "path")]
    pub path: String,


    #[arg(short = 't', long = "type")]
    pub socket_type: Option<String>,


    #[arg(short = 'b', long = "backlog", default_value = "128")]
    pub backlog: i32,


    #[arg(short = 's', long = "service")]
    pub service: Option<String>,
}

#[derive(Args)]
pub struct SocketDestroyArgs {

    #[arg(short = 'p', long = "path")]
    pub path: String,


    #[arg(short = 'f', long = "force")]
    pub force: bool,
}

#[derive(Args)]
pub struct SocketMonitorArgs {

    #[arg(short = 'p', long = "path")]
    pub path: String,


    #[arg(short = 'i', long = "interval", default_value = "1000")]
    pub interval: u64,


    #[arg(short = 'e', long = "events")]
    pub events: Option<String>,
}

#[derive(Args)]
pub struct SocketTraceArgs {

    #[arg(short = 'p', long = "path")]
    pub path: String,


    #[arg(short = 'd', long = "duration")]
    pub duration: Option<u64>,


    #[arg(short = 'f', long = "filter")]
    pub filter: Option<String>,
}

#[derive(Args)]
pub struct SocketActivateArgs {

    #[arg(short = 'p', long = "path")]
    pub path: String,


    #[arg(short = 's', long = "service")]
    pub service: Option<String>,


    #[arg(short = 'o', long = "one-shot")]
    pub one_shot: bool,
}

#[derive(Args)]
pub struct SocketQueryArgs {

    #[arg(short = 'p', long = "path")]
    pub path: String,


    #[arg(short = 'd', long = "data")]
    pub data: Option<String>,


    #[arg(short = 't', long = "timeout", default_value = "5")]
    pub timeout: u64,


    #[arg(short = 'n', long = "non-blocking")]
    pub non_blocking: bool,
}


#[derive(Subcommand)]
pub enum DaemonCommand {

    Start(DaemonStartArgs),


    Stop(DaemonStopArgs),


    Restart(DaemonRestartArgs),


    Status(DaemonStatusArgs),


    #[command(alias("ls"))]
    List(DaemonListArgs),


    Log(DaemonLogArgs),


    Install(DaemonInstallArgs),


    Uninstall(DaemonUninstallArgs),


    Update(DaemonUpdateArgs),


    Rollback(DaemonRollbackArgs),


    Pin(DaemonPinArgs),


    Trust(DaemonTrustArgs),
}

#[derive(Args)]
pub struct DaemonStartArgs {
    #[arg(short = 'n', long = "name")]
    pub name: String,
    #[arg(short = 'w', long = "wait")]
    pub wait: bool,
    #[arg(short = 't', long = "timeout", default_value = "30")]
    pub timeout: u64,
}

pub type DaemonStopArgs = ServiceForceTimeoutArgs;
pub type DaemonRestartArgs = ServiceForceTimeoutArgs;

#[derive(Args)]
pub struct DaemonStatusArgs {
    #[arg(short = 'n', long = "name")]
    pub name: Option<String>,
    #[arg(short = 'a', long = "all")]
    pub all: bool,
    #[arg(short = 'j', long = "json")]
    pub json: bool,
}

#[derive(Args)]
pub struct DaemonListArgs {
    #[arg(short = 'a', long = "all")]
    pub all: bool,
    #[arg(short = 'r', long = "running")]
    pub running: bool,
    #[arg(short = 'f', long = "failed")]
    pub failed: bool,
    #[arg(short = 'j', long = "json")]
    pub json: bool,
}

#[derive(Args)]
pub struct DaemonLogArgs {
    #[arg(short = 'n', long = "name")]
    pub name: String,
    #[arg(short = 'f', long = "follow")]
    pub follow: bool,
    #[arg(short = 'l', long = "lines", default_value = "50")]
    pub lines: usize,
}

#[derive(Args)]
pub struct DaemonInstallArgs {
    #[arg(short = 'n', long = "name")]
    pub name: String,
    #[arg(short = 'f', long = "from")]
    pub from: Option<String>,
    #[arg(short = 'e', long = "enable")]
    pub enable: bool,
    #[arg(short = 's', long = "start")]
    pub start: bool,
}

#[derive(Args)]
pub struct DaemonUninstallArgs {
    #[arg(short = 'n', long = "name")]
    pub name: String,
    #[arg(short = 's', long = "stop")]
    pub stop: bool,
    #[arg(short = 'p', long = "purge")]
    pub purge: bool,
}

#[derive(Args)]
pub struct DaemonUpdateArgs {
    #[arg(short = 'n', long = "name")]
    pub name: String,
    #[arg(short = 'f', long = "from")]
    pub from: Option<String>,
    #[arg(short = 'F', long = "force")]
    pub force: bool,
}

#[derive(Args)]
pub struct DaemonRollbackArgs {
    #[arg(short = 'n', long = "name")]
    pub name: String,
    #[arg(short = 'r', long = "to-revision")]
    pub to_revision: Option<String>,
}

#[derive(Args)]
pub struct DaemonPinArgs {
    #[arg(short = 'n', long = "name")]
    pub name: String,
    #[arg(short = 'v', long = "version")]
    pub version: Option<String>,
}

#[derive(Args)]
pub struct DaemonTrustArgs {
    #[arg(short = 'n', long = "name")]
    pub name: String,
    #[arg(short = 'k', long = "key")]
    pub key: Option<String>,
    #[arg(short = 'v', long = "verify")]
    pub verify: bool,
}


#[derive(Subcommand)]
pub enum SnapshotCommand {

    Create(SnapshotCreateArgs),


    #[command(alias("ls"))]
    List(SnapshotListArgs),


    Restore(SnapshotRestoreArgs),


    Delete(SnapshotDeleteArgs),


    Diff(SnapshotDiffArgs),


    Export(SnapshotExportArgs),


    Import(SnapshotImportArgs),
}

#[derive(Args)]
pub struct SnapshotCreateArgs {
    #[arg(short = 'n', long = "name")]
    pub name: String,
    #[arg(short = 'd', long = "description")]
    pub description: Option<String>,
    #[arg(short = 's', long = "services")]
    pub services: Option<String>,
    #[arg(short = 'l', long = "include-logs")]
    pub include_logs: bool,
}

#[derive(Args)]
pub struct SnapshotListArgs {
    #[arg(short = 'a', long = "all")]
    pub all: bool,
    #[arg(short = 'r', long = "recent")]
    pub recent: bool,
    #[arg(short = 'j', long = "json")]
    pub json: bool,
}

#[derive(Args)]
pub struct SnapshotRestoreArgs {
    #[arg(short = 'n', long = "name")]
    pub name: String,
    #[arg(short = 'f', long = "force")]
    pub force: bool,
    #[arg(short = 'd', long = "dry-run")]
    pub dry_run: bool,
}

#[derive(Args)]
pub struct SnapshotDeleteArgs {
    #[arg(short = 'n', long = "name")]
    pub name: String,
    #[arg(short = 'f', long = "force")]
    pub force: bool,
}

#[derive(Args)]
pub struct SnapshotDiffArgs {
    #[arg(short = 'f', long = "from")]
    pub from: String,
    #[arg(short = 't', long = "to")]
    pub to: String,
    #[arg(short = 's', long = "services")]
    pub services: Option<String>,
}

#[derive(Args)]
pub struct SnapshotExportArgs {
    #[arg(short = 'n', long = "name")]
    pub name: String,
    #[arg(short = 'o', long = "output")]
    pub output: String,
    #[arg(short = 'f', long = "format")]
    pub format: Option<String>,
}

#[derive(Args)]
pub struct SnapshotImportArgs {
    #[arg(short = 'f', long = "file")]
    pub file: String,
    #[arg(short = 'n', long = "name")]
    pub name: Option<String>,
    #[arg(short = 'F', long = "force")]
    pub force: bool,
}


#[derive(Subcommand)]
pub enum SecurityCommand {

    Audit(SecurityAuditArgs),


    Scan(SecurityScanArgs),


    Policy(SecurityPolicyArgs),


    Cap(SecurityCapArgs),


    Seccomp(SecuritySeccompArgs),


    Sandbox(SecuritySandboxArgs),


    Trust(SecurityTrustArgs),
}

#[derive(Args)]
pub struct SecurityAuditArgs {
    #[arg(short = 'a', long = "all")]
    pub all: bool,
    #[arg(short = 's', long = "services")]
    pub services: bool,
    #[arg(short = 'c', long = "config")]
    pub config: bool,
    #[arg(short = 'n', long = "network")]
    pub network: bool,
}

#[derive(Args)]
pub struct SecurityScanArgs {
    #[arg(short = 't', long = "targets")]
    pub targets: Option<String>,
    #[arg(short = 's', long = "severity")]
    pub severity: Option<String>,
    #[arg(short = 'f', long = "fix")]
    pub fix: bool,
}

#[derive(Args)]
pub struct SecurityPolicyArgs {
    #[arg(short = 's', long = "show")]
    pub show: bool,
    #[arg(short = 'S', long = "set")]
    pub set: Option<String>,
    #[arg(short = 'v', long = "validate")]
    pub validate: bool,
}

#[derive(Args)]
pub struct SecurityCapArgs {
    #[arg(short = 'l', long = "list")]
    pub list: bool,
    #[arg(short = 'a', long = "add")]
    pub add: Option<String>,
    #[arg(short = 'd', long = "drop")]
    pub drop: Option<String>,
    #[arg(short = 'p', long = "pid")]
    pub pid: Option<u32>,
}

#[derive(Args)]
pub struct SecuritySeccompArgs {
    #[arg(short = 'l', long = "list")]
    pub list: bool,
    #[arg(short = 'a', long = "apply")]
    pub apply: Option<String>,
    #[arg(short = 'd', long = "dump")]
    pub dump: Option<String>,
    #[arg(short = 'p', long = "pid")]
    pub pid: Option<u32>,
}

#[derive(Args)]
pub struct SecuritySandboxArgs {
    #[arg(short = 'c', long = "create")]
    pub create: Option<String>,
    #[arg(short = 'd', long = "destroy")]
    pub destroy: Option<String>,
    #[arg(short = 'l', long = "list")]
    pub list: bool,
    #[arg(short = 'i', long = "info")]
    pub info: Option<String>,
}

#[derive(Args)]
pub struct SecurityTrustArgs {
    #[arg(short = 'k', long = "keys")]
    pub keys: bool,
    #[arg(short = 'a', long = "add")]
    pub add: Option<String>,
    #[arg(short = 'r', long = "remove")]
    pub remove: Option<String>,
    #[arg(short = 'v', long = "verify")]
    pub verify: bool,
}


#[derive(Subcommand)]
pub enum QueryCommand {

    Service(QueryServiceArgs),


    Boot(QueryBootArgs),


    System(QuerySystemArgs),


    Dependency(QueryDependencyArgs),


    History(QueryHistoryArgs),


    Resource(QueryResourceArgs),


    Health(QueryHealthArgs),
}

#[derive(Args)]
pub struct QueryServiceArgs {
    #[arg(short = 'n', long = "name")]
    pub name: Option<String>,
    #[arg(short = 'f', long = "format")]
    pub format: Option<String>,
    #[arg(short = 'F', long = "fields")]
    pub fields: Option<String>,
    #[arg(short = 'j', long = "json")]
    pub json: bool,
}

#[derive(Args)]
pub struct QueryBootArgs {
    #[arg(short = 'p', long = "phase")]
    pub phase: Option<String>,
    #[arg(short = 't', long = "time")]
    pub time: bool,
    #[arg(short = 'e', long = "errors")]
    pub errors: bool,
    #[arg(short = 'f', long = "format")]
    pub format: Option<String>,
    #[arg(short = 'j', long = "json")]
    pub json: bool,
}

#[derive(Args)]
pub struct QuerySystemArgs {
    #[arg(short = 'a', long = "all")]
    pub all: bool,
    #[arg(short = 'f', long = "format")]
    pub format: Option<String>,
    #[arg(short = 'F', long = "fields")]
    pub fields: Option<String>,
    #[arg(short = 'j', long = "json")]
    pub json: bool,
}

#[derive(Args)]
pub struct QueryDependencyArgs {
    #[arg(short = 'n', long = "name")]
    pub name: Option<String>,
    #[arg(short = 'r', long = "reverse")]
    pub reverse: bool,
    #[arg(short = 'f', long = "format")]
    pub format: Option<String>,
    #[arg(short = 'g', long = "graph")]
    pub graph: bool,
}

#[derive(Args)]
pub struct QueryHistoryArgs {
    #[arg(short = 'l', long = "limit", default_value = "20")]
    pub limit: usize,
    #[arg(short = 'f', long = "format")]
    pub format: Option<String>,
    #[arg(short = 'e', long = "events")]
    pub events: Option<String>,
    #[arg(short = 'j', long = "json")]
    pub json: bool,
}

#[derive(Args)]
pub struct QueryResourceArgs {
    #[arg(short = 't', long = "type")]
    pub resource_type: Option<String>,
    #[arg(short = 'i', long = "interval")]
    pub interval: Option<u64>,
    #[arg(short = 'f', long = "format")]
    pub format: Option<String>,
    #[arg(short = 'j', long = "json")]
    pub json: bool,
}

#[derive(Args)]
pub struct QueryHealthArgs {
    #[arg(short = 'a', long = "all")]
    pub all: bool,
    #[arg(short = 's', long = "service")]
    pub service: Option<String>,
    #[arg(short = 'f', long = "format")]
    pub format: Option<String>,
    #[arg(short = 'j', long = "json")]
    pub json: bool,
}


#[derive(Subcommand)]
pub enum DebugCommand {

    Trace(DebugTraceArgs),


    Strace(DebugStraceArgs),


    Dump(DebugDumpArgs),


    Core(DebugCoreArgs),


    Profile(DebugProfileArgs),


    Stress(DebugStressArgs),


    Test(DebugTestArgs),
}

#[derive(Args)]
pub struct DebugTraceArgs {
    #[arg(short = 'p', long = "pid")]
    pub pid: u32,
    #[arg(short = 's', long = "signal")]
    pub signal: Option<String>,
    #[arg(short = 'o', long = "output")]
    pub output: Option<String>,
}

#[derive(Args)]
pub struct DebugStraceArgs {
    #[arg(short = 'p', long = "pid")]
    pub pid: u32,
    #[arg(short = 'f', long = "filter")]
    pub filter: Option<String>,
    #[arg(short = 'o', long = "output")]
    pub output: Option<String>,
    #[arg(short = 't', long = "timeout")]
    pub timeout: Option<u64>,
}

#[derive(Args)]
pub struct DebugDumpArgs {
    #[arg(short = 'a', long = "all")]
    pub all: bool,
    #[arg(short = 's', long = "services")]
    pub services: bool,
    #[arg(short = 'f', long = "format")]
    pub format: Option<String>,
    #[arg(short = 'o', long = "output")]
    pub output: Option<String>,
}

#[derive(Args)]
pub struct DebugCoreArgs {
    #[arg(short = 'p', long = "pid")]
    pub pid: u32,
    #[arg(short = 'o', long = "output")]
    pub output: Option<String>,
    #[arg(short = 'l', long = "limit")]
    pub limit: Option<usize>,
}

#[derive(Args)]
pub struct DebugProfileArgs {
    #[arg(short = 'p', long = "pid")]
    pub pid: u32,
    #[arg(short = 'd', long = "duration", default_value = "10")]
    pub duration: u64,
    #[arg(short = 'f', long = "frequency", default_value = "99")]
    pub frequency: u32,
    #[arg(short = 'o', long = "output")]
    pub output: Option<String>,
}

#[derive(Args)]
pub struct DebugStressArgs {
    #[arg(short = 'c', long = "cpu")]
    pub cpu: bool,
    #[arg(short = 'm', long = "memory")]
    pub memory: bool,
    #[arg(short = 'i', long = "io")]
    pub io: bool,
    #[arg(short = 'd', long = "duration", default_value = "30")]
    pub duration: u64,
}

#[derive(Args)]
pub struct DebugTestArgs {
    #[arg(short = 'm', long = "module")]
    pub module: Option<String>,
    #[arg(short = 'a', long = "all")]
    pub all: bool,
    #[arg(short = 'v', long = "verbose")]
    pub verbose: bool,
}


#[derive(Subcommand)]
pub enum SelfCommand {

    Status(SelfStatusArgs),


    Update(SelfUpdateArgs),


    Version(SelfVersionArgs),


    Completions(SelfCompletionsArgs),


    Config(SelfConfigArgs),

}

#[derive(Args)]
pub struct SelfStatusArgs {
    #[arg(short = 'v', long = "verbose")]
    pub verbose: bool,
}

#[derive(Args)]
pub struct SelfUpdateArgs {
    #[arg(short = 'f', long = "force")]
    pub force: bool,


    #[arg(short = 'c', long = "check")]
    pub check: bool,


    #[arg(short = 'C', long = "channel")]
    pub channel: Option<String>,
}

#[derive(Args)]
pub struct SelfVersionArgs {
    #[arg(short = 'j', long = "json")]
    pub json: bool,

    #[arg(short = 's', long = "short")]
    pub short: bool,
}

#[derive(Args)]
pub struct SelfCompletionsArgs {

    #[arg(short = 's', long = "shell")]
    pub shell: Option<String>,


    #[arg(short = 'o', long = "output")]
    pub output: Option<String>,
}

#[derive(Args)]
pub struct SelfConfigArgs {
    #[arg(short = 's', long = "show")]
    pub show: bool,

    #[arg(short = 'S', long = "set")]
    pub set: Option<String>,

    #[arg(short = 'g', long = "get")]
    pub get: Option<String>,
}


#[derive(Subcommand)]
pub enum PluginCommand {

    List,

    Run(PluginRunArgs),

    Install(PluginInstallArgs),

    Remove(PluginRemoveArgs),

    Info(PluginInfoArgs),
}

#[derive(Args)]
pub struct PluginRunArgs {
    pub alias: String,

    #[arg(allow_hyphen_values = true, trailing_var_arg = true)]
    pub args: Vec<String>,
}

#[derive(Args)]
pub struct PluginInstallArgs {
    pub path: String,

    #[arg(short = 'n', long = "name")]
    pub name: Option<String>,

    #[arg(short = 'a', long = "alias")]
    pub alias: Option<String>,

    #[arg(short = 'A', long = "aliases", value_parser = parse_key_val)]
    pub aliases: Vec<(String, String)>,

    #[arg(short = 'f', long = "force")]
    pub force: bool,
}

#[derive(Args)]
pub struct PluginRemoveArgs {
    pub name: String,
}

#[derive(Args)]
pub struct PluginInfoArgs {
    pub name: String,
}


#[derive(Subcommand)]
pub enum ThemeCommand {

    List,

    Apply(ThemeApplyArgs),

    Install(ThemeInstallArgs),

    Remove(ThemeRemoveArgs),

    Info(ThemeInfoArgs),
}

#[derive(Args)]
pub struct ThemeApplyArgs {
    pub name: String,
}

#[derive(Args)]
pub struct ThemeInstallArgs {
    pub path: String,

    #[arg(short = 'n', long = "name")]
    pub name: Option<String>,

    #[arg(short = 'f', long = "force")]
    pub force: bool,
}

#[derive(Args)]
pub struct ThemeRemoveArgs {
    pub name: String,
}

#[derive(Args)]
pub struct ThemeInfoArgs {
    pub name: String,
}

#[derive(Subcommand)]
pub enum TuiCommand {

    List,

    Apply(TuiApplyArgs),

    Install(TuiInstallArgs),

    Remove(TuiRemoveArgs),

    Info(TuiInfoArgs),
}

#[derive(Args)]
pub struct TuiApplyArgs {
    pub name: String,
}

#[derive(Args)]
pub struct TuiInstallArgs {
    pub path: String,

    #[arg(short = 'n', long = "name")]
    pub name: Option<String>,

    #[arg(short = 'f', long = "force")]
    pub force: bool,
}

#[derive(Args)]
pub struct TuiRemoveArgs {
    pub name: String,
}

#[derive(Args)]
pub struct TuiInfoArgs {
    pub name: String,
}

fn parse_key_val(s: &str) -> Result<(String, String), String> {
    let mut parts = s.splitn(2, '=');
    let key = parts.next().ok_or("missing key")?.to_string();
    let val = parts.next().ok_or("missing value")?.to_string();
    Ok((key, val))
}
