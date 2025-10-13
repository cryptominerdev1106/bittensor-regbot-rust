use anyhow::{anyhow, Result};
use clap::{Parser, Subcommand};
use colored::*;
use std::time::Duration;
use tokio::time::sleep;

pub mod client;
pub mod constants;
pub mod key_utils;
pub mod register;
pub mod utils;

use crate::register::*;

#[derive(Parser)]
#[command(name = "bittensor-quick-register")]
#[command(about = "Quick registration tool for Bittensor network")]
struct Cli {
    /// RPC endpoint URL
    /// One or more RPC endpoints (repeat flag to add more)
    #[arg(
        short = 'r',
        long = "rpc-url",
        num_args = 1..,
        value_delimiter = ',',
        default_values_t = vec![String::from("wss://entrypoint-finney.opentensor.ai:443")]
    )]
    rpc_url: Vec<String>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Register to a subnet using burn registration
    Register {
        #[arg(short, long)]
        subnet: u16,
        #[arg(short, long)]
        wallet: String,
        #[arg(short = 'H', long)]
        hotkey: String,
        #[arg(long)]
        burn_amount: Option<u64>,
        /// Wait for next block then submit (helps land at front of next block)
        #[arg(long, default_value_t = false)]
        submit_on_new_head: bool,
        /// Delay after new head before first submit (ms)
        #[arg(long, default_value_t = 250)]
        head_delay_ms: u64,
        /// Mortal era period in blocks (default 64)
        #[arg(long, default_value_t = 64)]
        era_period: u64,

        /// Watch mempool for competing burned_register txs
        #[arg(long, default_value_t = false)]
        watch_mempool: bool,
        /// How long to watch before first submit (seconds)
        #[arg(long, default_value_t = 0)]
        watch_duration_secs: u64,
        /// Poll interval while watching (ms)
        #[arg(long, default_value_t = 500)]
        watch_interval_ms: u64,
        /// If true, bump the first tip immediately when competition is detected
        #[arg(long, default_value_t = false)]
        watch_reactive: bool,
        /// Multiplicative bump to apply when competition detected (e.g., 1.25)
        #[arg(long, default_value_t = 1.25)]
        watch_bump_now: f64,

        /// Optional tip in RAO (planck) to prioritize inclusion
        #[arg(long)]
        tip: Option<u128>,
        /// RBF rounds (same nonce, higher tip)
        #[arg(long, default_value_t = 3)]
        rbf_rounds: u32,
        /// Tip bump factor (e.g., 1.5)
        #[arg(long, default_value_t = 1.5)]
        bump: f64,
        /// Seconds to wait between RBF resubmissions
        #[arg(long, default_value_t = 6)]
        rbf_wait_secs: u64,
        /// Optional tip in RAO (planck) to prioritize inclusion
        #[arg(long)]
        tip: Option<u128>,
        /// RBF rounds (same nonce, higher tip)
        #[arg(long, default_value_t = 3)]
        rbf_rounds: u32,
        /// Tip bump factor (e.g., 1.5)
        #[arg(long, default_value_t = 1.5)]
        bump: f64,
        /// Seconds to wait between RBF resubmissions
        #[arg(long, default_value_t = 6)]
        rbf_wait_secs: u64,
    },

    /// Check registration status of a hotkey
    Status {
        #[arg(short, long)]
        subnet: u16,
        #[arg(short = 'H', long)]
        hotkey: String,
    },

    /// Show detailed subnet information
    SubnetInfo {
        #[arg(short, long)]
        subnet: u16,
    },

    /// Estimate registration costs and time
    EstimateCost {
        #[arg(short, long)]
        subnet: u16,
    },

    /// Monitor multiple neurons across subnets
    Monitor {
        #[arg(short, long, help = "Format: subnet1:hotkey1,subnet2:hotkey2")]
        neurons: Vec<String>,
        #[arg(long, default_value = "60")]
        interval: u64,
    },

    /// Auto-register with retry logic
    AutoRegister {
        #[arg(short, long)]
        subnet: u16,
        #[arg(short, long)]
        wallet: String,
        #[arg(short = 'H', long)]
        hotkey: String,
        #[arg(long, default_value = "3")]
        max_retries: usize,
    },

    /// Show network statistics
    NetworkStats,

    /// Export subnet configuration
    ExportConfig {
        #[arg(short, long)]
        subnet: u16,
        #[arg(short, long, default_value = "subnet_config.json")]
        output: String,
    },

    /// Batch operations from config file
    Batch {
        #[arg(short, long)]
        config: String,
    },

    /// Check account balance
    Balance {
        #[arg(short, long)]
        account: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp(None)
        .format_module_path(false)
        .init();

    print_banner();

    let cli = Cli::parse();

    match cli.command {
        Commands::Register {
            subnet,
            wallet,
            hotkey,
            burn_amount,
            submit_on_new_head,
            head_delay_ms,
            era_period,
            tip,
            rbf_rounds,
            bump,
            rbf_wait_secs,
        } => {
            let register_client: QuickRegister = QuickRegister::new(cli.rpc_url.clone()).await?;
            register_client
                .register_to_subnet(subnet, &wallet, &hotkey, burn_amount, submit_on_new_head, head_delay_ms, era_period, tip, rbf_rounds, bump, rbf_wait_secs)
                .await?;
        }

        Commands::Status { subnet, hotkey } => {
            let register_client = QuickRegister::new(cli.rpc_url).await?;
            register_client.check_status(subnet, &hotkey).await?;
        }

        Commands::SubnetInfo { subnet } => {
            let register_client = QuickRegister::new(cli.rpc_url).await?;
            register_client.show_subnet_info(subnet).await?;
        }

        Commands::EstimateCost { subnet } => {
            let register_client = QuickRegister::new(cli.rpc_url).await?;
            register_client.estimate_registration_cost(subnet).await?;
        }

        Commands::Monitor { neurons, interval } => {
            let register_client = QuickRegister::new(cli.rpc_url).await?;
            let parsed_neurons: Result<Vec<(u16, String)>> = neurons
                .iter()
                .map(|s| {
                    let parts: Vec<&str> = s.split(':').collect();
                    if parts.len() == 2 {
                        Ok((parts[0].parse::<u16>()?, parts[1].to_string()))
                    } else {
                        Err(anyhow!("Invalid format: {}. Use subnet:hotkey", s))
                    }
                })
                .collect();

            let parsed_neurons = parsed_neurons?;

            loop {
                register_client
                    .monitor_multiple_neurons(parsed_neurons.clone())
                    .await?;
                println!("\n⏳ Waiting {}s before next check...", interval);
                sleep(Duration::from_secs(interval)).await;
            }
        }

        Commands::AutoRegister {
            subnet,
            wallet,
            hotkey,
            max_retries,
        } => {
            let register_client = QuickRegister::new(cli.rpc_url).await?;
            register_client
                .auto_register_with_retry(subnet, &wallet, &hotkey, max_retries)
                .await?;
        }

        Commands::NetworkStats => {
            let register_client = QuickRegister::new(cli.rpc_url).await?;
            register_client.show_network_statistics().await?;
        }

        Commands::ExportConfig { subnet, output } => {
            let register_client = QuickRegister::new(cli.rpc_url).await?;
            register_client.export_config(subnet, &output).await?;
        }

        Commands::Batch { config } => {
            let register_client = QuickRegister::new(cli.rpc_url).await?;
            register_client.execute_batch_operations(&config).await?;
        }

        Commands::Balance { account } => {
            let register_client = QuickRegister::new(cli.rpc_url).await?;
            register_client.check_account_balance(&account).await?;
        }
    }

    Ok(())
}

fn print_banner() {
    println!(
        "{}",
        r#"
 ╔═══════════════════════════════════════════════════════════╗
 ║                                                           ║
 ║    🚀 Bittensor Quick Register v0.2.0                     ║
 ║    ⚡ Fast • Reliable • Burn Registration                 ║
 ║                                                           ║
 ╚═══════════════════════════════════════════════════════════╝
    "#
        .bright_cyan()
    );
}
