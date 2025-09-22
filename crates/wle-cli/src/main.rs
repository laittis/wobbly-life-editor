use clap::{Args as ClapArgs, Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "wle-cli",
    about = "Dump and edit Wobbly Life saves via generic JSON Pointer API",
    version
)]
struct Cli {
    #[command(subcommand)]
    cmd: Option<Cmd>,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// Dump a file or directory as JSON
    Dump(DumpArgs),
    /// Get value at JSON pointer
    Get(EditArgs),
    /// List children at JSON pointer
    List(EditArgs),
    /// Set value (raw JSON) at JSON pointer; prints or writes with --out
    Set(SetArgs),
    /// Remove key or array element at JSON pointer; prints or writes with --out
    Remove(RemoveArgs),
    /// Write a JSON file (produced by dump) back to a BinaryFormatter .sav
    Write(WriteArgs),
}

#[derive(ClapArgs, Debug)]
struct DumpArgs {
    /// File or directory to dump (defaults to reference-data/GameSaves/SaveSlot_1)
    path: Option<PathBuf>,
    /// Max array elements to include per array
    #[arg(long, default_value_t = 128)]
    max_array: usize,
    /// Max recursion depth
    #[arg(long, default_value_t = 16)]
    max_depth: usize,
    /// Emit full bytes instead of summaries
    #[arg(long, default_value_t = false)]
    bytes_full: bool,
}

#[derive(ClapArgs, Debug)]
struct EditArgs {
    /// File to load (.sav or .json)
    path: PathBuf,
    /// JSON Pointer, e.g. /root/some/key
    #[arg(long)]
    ptr: String,
    /// Max array elements to include per array
    #[arg(long, default_value_t = 128)]
    max_array: usize,
    /// Max recursion depth
    #[arg(long, default_value_t = 16)]
    max_depth: usize,
}

#[derive(ClapArgs, Debug)]
struct SetArgs {
    /// File to load (.sav or .json)
    path: PathBuf,
    /// JSON Pointer, e.g. /root/some/key
    #[arg(long)]
    ptr: String,
    /// New value as raw JSON (e.g., 123, true, "str", {"a":1})
    #[arg(long)]
    value: String,
    /// Optional output .json path to write; otherwise prints to stdout
    #[arg(long)]
    out: Option<PathBuf>,
    /// Max array elements to include per array
    #[arg(long, default_value_t = 128)]
    max_array: usize,
    /// Max recursion depth
    #[arg(long, default_value_t = 16)]
    max_depth: usize,
}

#[derive(ClapArgs, Debug)]
struct RemoveArgs {
    /// File to load (.sav or .json)
    path: PathBuf,
    /// JSON Pointer, e.g. /root/some/key or /root/arr/2
    #[arg(long)]
    ptr: String,
    /// Optional output .json path to write; otherwise prints to stdout
    #[arg(long)]
    out: Option<PathBuf>,
    /// Max array elements to include per array
    #[arg(long, default_value_t = 128)]
    max_array: usize,
    /// Max recursion depth
    #[arg(long, default_value_t = 16)]
    max_depth: usize,
}

#[derive(ClapArgs, Debug)]
struct WriteArgs {
    /// Input JSON path (from dump)
    #[arg(long, value_name = "JSON")]
    input: PathBuf,
    /// Output .sav path
    #[arg(long, value_name = "SAV")]
    output: PathBuf,
}

fn main() {
    let cli = Cli::parse();
    match cli.cmd.unwrap_or(Cmd::Dump(DumpArgs {
        path: Some(PathBuf::from("reference-data/GameSaves/SaveSlot_1")),
        max_array: 128,
        max_depth: 16,
        bytes_full: false,
    })) {
        Cmd::Dump(a) => cmd_dump(a),
        Cmd::Get(a) => cmd_get(a),
        Cmd::List(a) => cmd_list(a),
        Cmd::Set(a) => cmd_set(a),
        Cmd::Remove(a) => cmd_remove(a),
        Cmd::Write(a) => cmd_write(a),
    }
}

fn cmd_dump(args: DumpArgs) {
    let path = args
        .path
        .unwrap_or_else(|| PathBuf::from("reference-data/GameSaves/SaveSlot_1"));
    let opts = wle_core::json::JsonOpts {
        max_array_elems: args.max_array,
        max_depth: args.max_depth,
        bytes_summary: !args.bytes_full,
    };
    let p = path.as_path();
    let res = if p.is_file() {
        wle_core::json::dump_file_json(p, opts)
    } else if p.is_dir() {
        wle_core::json::dump_dir_map_json(p, opts)
    } else {
        Err(format!("not found: {}", p.display()))
    };
    match res {
        Ok(s) => print!("{}", s),
        Err(e) => {
            eprintln!("error: {}", e);
            std::process::exit(2);
        }
    }
}

fn cmd_get(args: EditArgs) {
    let opts = wle_core::json::JsonOpts {
        max_array_elems: args.max_array,
        max_depth: args.max_depth,
        bytes_summary: true,
    };
    let v = wle_core::parse_file_to_json_value(&args.path, opts).unwrap_or_else(|e| {
        eprintln!("error: {}", e);
        std::process::exit(2);
    });
    match wle_core::get_by_pointer(&v, &args.ptr) {
        Some(x) => println!("{}", serde_json::to_string_pretty(&x).unwrap()),
        None => {
            eprintln!("not found: {}", args.ptr);
            std::process::exit(3);
        }
    }
}

fn cmd_list(args: EditArgs) {
    let opts = wle_core::json::JsonOpts {
        max_array_elems: args.max_array,
        max_depth: args.max_depth,
        bytes_summary: true,
    };
    let v = wle_core::parse_file_to_json_value(&args.path, opts).unwrap_or_else(|e| {
        eprintln!("error: {}", e);
        std::process::exit(2);
    });
    match wle_core::list_children(&v, &args.ptr) {
        Ok(children) => {
            for c in children {
                println!(
                    "{}\t{:?}{}",
                    c.key_or_index,
                    c.kind,
                    c.len.map(|n| format!("\t(len={})", n)).unwrap_or_default()
                );
            }
        }
        Err(e) => {
            eprintln!("error: {}", e);
            std::process::exit(3);
        }
    }
}

fn cmd_set(args: SetArgs) {
    let opts = wle_core::json::JsonOpts {
        max_array_elems: args.max_array,
        max_depth: args.max_depth,
        bytes_summary: true,
    };
    let mut v = wle_core::parse_file_to_json_value(&args.path, opts).unwrap_or_else(|e| {
        eprintln!("error: {}", e);
        std::process::exit(2);
    });
    let new_val: serde_json::Value = serde_json::from_str(&args.value).unwrap_or_else(|e| {
        eprintln!("invalid --value JSON: {}", e);
        std::process::exit(3);
    });
    wle_core::set_raw_by_pointer(&mut v, &args.ptr, new_val).unwrap_or_else(|e| {
        eprintln!("error: {}", e);
        std::process::exit(4);
    });
    if let Some(out) = args.out {
        wle_core::write_json_to_file(&out, &v).unwrap_or_else(|e| {
            eprintln!("error writing: {}", e);
            std::process::exit(5);
        });
    } else {
        println!("{}", serde_json::to_string_pretty(&v).unwrap());
    }
}

fn cmd_remove(args: RemoveArgs) {
    let opts = wle_core::json::JsonOpts {
        max_array_elems: args.max_array,
        max_depth: args.max_depth,
        bytes_summary: true,
    };
    let mut v = wle_core::parse_file_to_json_value(&args.path, opts).unwrap_or_else(|e| {
        eprintln!("error: {}", e);
        std::process::exit(2);
    });
    wle_core::remove_at_pointer(&mut v, &args.ptr).unwrap_or_else(|e| {
        eprintln!("error: {}", e);
        std::process::exit(4);
    });
    if let Some(out) = args.out {
        wle_core::write_json_to_file(&out, &v).unwrap_or_else(|e| {
            eprintln!("error writing: {}", e);
            std::process::exit(5);
        });
    } else {
        println!("{}", serde_json::to_string_pretty(&v).unwrap());
    }
}

fn cmd_write(args: WriteArgs) {
    let data = std::fs::read_to_string(&args.input).unwrap_or_else(|e| {
        eprintln!("error reading JSON: {}", e);
        std::process::exit(2);
    });
    let value: serde_json::Value = serde_json::from_str(&data).unwrap_or_else(|e| {
        eprintln!("invalid JSON: {}", e);
        std::process::exit(3);
    });
    wle_core::write_binfmt_file_from_json(&args.output, &value).unwrap_or_else(|e| {
        eprintln!("write error: {}", e);
        std::process::exit(4);
    });
}
