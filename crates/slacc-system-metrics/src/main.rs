use clap::Parser;
use miette::Result;
use slacc_system_metrics::*;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
pub struct MisskeyStatsArguments {
    /// Display information about disk.
    #[arg(long, default_value_t = false)]
    disk_io: bool,
    /// Display information about memory.
    #[arg(long, default_value_t = false)]
    memory_info: bool,
    /// Display information about network.
    #[arg(long, default_value_t = false)]
    network_info: bool,
    /// Display information about disk space.
    #[arg(long, default_value_t = false)]
    disk_space_info: bool,
}

fn main() -> Result<()> {
    let args = MisskeyStatsArguments::parse();

    if args.disk_io {
        let DiskInformation {
            read_count,
            write_count,
        } = get_disk_io()?;
        println!("Operations (Read): {}", read_count);
        println!("Operations (Write): {}", write_count);
    }

    if args.memory_info {
        let MemoryInformation {
            used_count,
            active_count,
            total_count,
        } = get_memory_info()?;
        println!("Memory (Used): {}", used_count);
        println!("Memory (Total): {}", total_count);
        println!("Memory (Active): {}", active_count);
    }

    if args.network_info {
        let NetworkInformation {
            read_bytes,
            write_bytes,
        } = get_network_info()?;
        println!("Network (Read): {}", read_bytes);
        println!("Network (Write): {}", write_bytes);
    }

    if args.disk_space_info {
        let DiskSpaceInformation {
            free_bytes,
            total_bytes,
        } = get_disk_space()?;
        println!("Disk (Free): {}", free_bytes);
        println!("Disk (Total): {}", total_bytes);
    }

    Ok(())
}
