//! Clap definitions for the `leekwars` binary.

use std::path::PathBuf;

use clap::{Parser, Subcommand};

use crate::batch::ui::ColorChoice;

#[derive(Parser)]
#[command(
    name = "leekwars",
    version,
    about = "Leek Wars CLI (unofficial; respect site terms and rate limits)",
    after_long_help = "Authentication (in order):\n  \
        1) --login / --password (or LEEKWARS_LOGIN / LEEKWARS_PASSWORD from the environment; .env is loaded)\n  \
        2) TOML config: ./leekwars.toml, or $LEEKWARS_CONFIG, or ~/.config/leekwars/config.toml\n\n\
        Example leekwars.toml (multiple accounts):\n  \
        default_profile = \"main\"\n\n  \
        [accounts.main]\n  \
        login = \"user1\"\n  \
        password = \"secret\"\n\n  \
        [accounts.alt]\n  \
        login = \"user2\"\n  \
        password = \"other\"\n\n\
        Single account shorthand (stored as profile `default`):\n  \
        login = \"user\"\n  \
        password = \"secret\"\n\n\
        Use `leekwars profiles` to list account names. \
        Do not commit real passwords."
)]
pub struct Cli {
    /// Print raw JSON instead of tables / summaries.
    #[arg(long, global = true)]
    pub json: bool,

    /// Login (overrides TOML). Same as env `LEEKWARS_LOGIN`.
    #[arg(long, global = true, env = "LEEKWARS_LOGIN")]
    pub login: Option<String>,

    /// Password (overrides TOML). Same as env `LEEKWARS_PASSWORD`.
    #[arg(long, global = true, env = "LEEKWARS_PASSWORD")]
    pub password: Option<String>,

    /// Account name: use `[accounts.NAME]` from the TOML config (when login/password are not set).
    #[arg(long, global = true, value_name = "NAME")]
    pub profile: Option<String>,

    /// TOML config path (default: ./leekwars.toml, then `$LEEKWARS_CONFIG`, then `~/.config/leekwars/config.toml`).
    #[arg(long, global = true, value_name = "PATH")]
    pub config: Option<PathBuf>,

    /// When to color batch / table output (`auto` = only if stderr is a TTY).
    #[arg(long, global = true, value_enum, default_value_t = ColorChoice::Auto)]
    pub color: ColorChoice,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// Log in and print session summary (farmer id, name).
    Login,
    /// List account names from the TOML config (`--config` / search path).
    Profiles,
    /// Print the game data bundle version (`data/version`). No login.
    DataVersion,
    /// Public farmer profile (`farmer/get/{id}`). No login.
    Farmer {
        farmer_id: i64,
    },
    /// Export all AIs to a directory (same layout as the web editor).
    AiExport {
        #[arg(short, long, default_value = "ai")]
        dir: PathBuf,
    },
    /// Download one AI by id to a file (or stdout).
    AiDownload {
        ai_id: i64,
        #[arg(short, long)]
        out: Option<PathBuf>,
    },
    /// Upload source to an existing AI. Use `--merge` to expand `include()` like `leekscript merge`.
    AiUpload {
        ai_id: i64,
        /// Source file (entry for `--merge`).
        path: PathBuf,
        #[arg(long)]
        merge: bool,
        /// Project root for includes (default: parent directory of `path`).
        #[arg(long)]
        merge_root: Option<PathBuf>,
    },
    /// Garden state and opponents (solo or farmer).
    Garden {
        /// If set, list solo opponents for this leek; otherwise farmer opponents.
        #[arg(long)]
        leek: Option<i64>,
    },
    /// Start one fight in the garden.
    Fight {
        #[command(subcommand)]
        action: FightAction,
    },
    /// Encyclopedia: fetch index, search, or open one page.
    Encyclopedia {
        #[command(subcommand)]
        cmd: EncyclopediaCmd,
    },
    /// Show a leek profile or only equipment.
    Leek {
        leek_id: i64,
        #[arg(long)]
        equipment_only: bool,
    },
    /// Farmer inventory (weapons, chips, potions, hats from session).
    Inventory,
    /// Attach or remove equipment (same semantics as the website).
    Equipment {
        #[command(subcommand)]
        action: EquipmentAction,
    },
    /// Leek build tools: export build to TOML, heuristic stat/component optimization, mirror another leek.
    Build {
        #[command(subcommand)]
        cmd: BuildCmd,
    },
    /// Batch garden fights from a TOML plan (smart/random opponent pick, delays, JSONL log, stats).
    /// Boss garden fights are not supported (the game uses WebSocket for those).
    Batch {
        #[command(subcommand)]
        cmd: BatchCmd,
    },
}

#[derive(Subcommand)]
pub enum BuildCmd {
    /// Download `leek/get` (and for a farmer, each leek) into a structured TOML file. No login required.
    Export {
        /// Single leek id (mutually exclusive with `--farmer`).
        #[arg(long, conflicts_with = "farmer")]
        leek: Option<i64>,
        /// Farmer id — fetches every leek on that account.
        #[arg(long, conflicts_with = "leek")]
        farmer: Option<i64>,
        /// Output path (default: stdout).
        #[arg(short, long)]
        out: Option<PathBuf>,
        /// Skip `data/get-all` (no item/component names in TOML).
        #[arg(long)]
        no_game_data: bool,
    },
    /// Compare another leek's components to your inventory; suggest crafting. Game data is fetched automatically.
    Mirror {
        /// Target leek id (`leek/get`).
        #[arg(long)]
        leek: i64,
        /// Do not log in — only list target components and craft recipes (no “missing vs your stock”).
        #[arg(long)]
        no_inventory: bool,
    },
    /// Heuristic search: pick components (from an allowed set) + allocate capital to approach target totals.
    /// Uses weighted MSE vs targets; not guaranteed globally optimal.
    Optimize {
        /// Copy targets from this leek's `total_*` stats (`leek/get`).
        #[arg(long, conflicts_with = "totals")]
        target_leek: Option<i64>,
        /// Comma-separated `stat=value` totals (e.g. `life=5000,strength=800`). Use with `--level`.
        #[arg(long, conflicts_with = "target_leek")]
        totals: Option<String>,
        /// Leek level (base stats). Required if `--totals` is set.
        #[arg(long)]
        level: Option<i64>,
        /// Capital budget to spend (same units as in-game capital).
        #[arg(long)]
        capital: i64,
        /// Comma-separated `stat=weight` for the objective (default: all 1.0).
        #[arg(long)]
        weights: Option<String>,
        /// Allow every component item template from game data (can be slow / huge search space).
        #[arg(long)]
        all_component_templates: bool,
        /// Comma-separated component **item template** ids you may use (ignored if `--all-component-templates`).
        #[arg(long)]
        allow_components: Option<String>,
        #[arg(long, default_value_t = 32u32)]
        restarts: u32,
        #[arg(long, default_value_t = 800u32)]
        hill_steps: u32,
        #[arg(long)]
        seed: Option<u64>,
    },
    /// Log in, **back up** your leek’s current loadout to TOML, then strip and re-equip to match `--target` (weapons, chips, hat, components). Does **not** change stats/capital. Requires enough items in stash (including what you already wear; they return to inventory when stripped).
    Apply {
        /// Your leek id (must belong to the logged-in account).
        #[arg(long)]
        leek: i64,
        /// Leek to copy equipment from (`leek/get`).
        #[arg(long)]
        target: i64,
        /// Backup directory (default: XDG data `…/leekwars/backups`).
        #[arg(long)]
        backup_dir: Option<PathBuf>,
        /// Check stash + current loadout against the target layout without changing anything.
        #[arg(long)]
        dry_run: bool,
        /// Required to perform the apply (not needed for `--dry-run`).
        #[arg(long)]
        yes: bool,
    },
}

#[derive(Subcommand)]
pub enum BatchCmd {
    /// Run fights from a TOML file (`quota`, `weights`, `delay_secs`, `delay_jitter_secs`, …).
    Run {
        /// Path to the batch plan (TOML).
        #[arg(short, long)]
        config: PathBuf,
        /// Resolve and print the plan only (no login, no fights). Use with `--json` for machine-readable output.
        #[arg(long)]
        dry_run: bool,
        /// Extra detail: per-fight remaining quota (`-v`), repeat for more (reserved).
        #[arg(short, long, action = clap::ArgAction::Count)]
        verbose: u8,
        /// No human terminal output except errors (stats file, JSONL log, and `[batch …]` lines are all off).
        #[arg(short, long)]
        quiet: bool,
        /// Hide per-fight lines; the final summary is still printed unless `--quiet`.
        #[arg(long)]
        no_progress: bool,
        /// Override `max_fights` from the batch TOML (cap total fights for this run).
        #[arg(long)]
        max_fights: Option<u32>,
    },
    /// Show opponent win/loss stats used for smart picking.
    Stats {
        #[arg(short, long, value_name = "PATH")]
        stats: Option<PathBuf>,
    },
    /// Clear stored opponent stats (does not delete the JSONL fight log).
    Reset {
        #[arg(short, long, value_name = "PATH")]
        stats: Option<PathBuf>,
        #[arg(long)]
        yes: bool,
    },
}

#[derive(Subcommand)]
pub enum FightAction {
    /// Solo: your leek vs another leek id.
    Solo { leek_id: i64, target_id: i64 },
    /// Farmer vs farmer.
    Farmer { target_id: i64 },
    /// Fetch fight JSON (`fight/get`) or logs (`fight/get-logs` with `--logs`). No login.
    Get {
        fight_id: i64,
        #[arg(long)]
        logs: bool,
    },
}

#[derive(Subcommand)]
pub enum EncyclopediaCmd {
    /// Download the full locale index (slug → metadata) as JSON.
    Fetch {
        locale: String,
        #[arg(short, long)]
        out: Option<PathBuf>,
    },
    /// Search page titles/slugs (substring, case-insensitive).
    Search { locale: String, query: String },
    /// Load one page by slug (e.g. `leek wars` — same key as in the index).
    Page { locale: String, slug: String },
}

#[derive(Subcommand)]
pub enum EquipmentAction {
    AddWeapon {
        leek_id: i64,
        /// Inventory weapon id (from `inventory --json`).
        weapon_id: i64,
    },
    RemoveWeapon {
        /// Instance id on the leek (from `leek --json`).
        weapon_id: i64,
    },
    AddChip {
        leek_id: i64,
        chip_id: i64,
    },
    RemoveChip {
        chip_id: i64,
    },
    SetHat {
        leek_id: i64,
        hat_template_id: i64,
    },
    RemoveHat {
        leek_id: i64,
    },
}
