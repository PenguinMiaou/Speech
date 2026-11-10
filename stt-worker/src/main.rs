mod diarize;
mod engine;
mod eval;
mod isolate;
mod serve;

use std::io::{stdin, stdout, BufReader, BufWriter};
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("serve") => {
            transcribe_cpp::disable_logging();
            let mut reader = BufReader::new(stdin().lock());
            let mut writer = BufWriter::new(stdout().lock());
            serve::serve(&mut reader, &mut writer, &mut engine::RealEngine::default()).map_err(|e| e.to_string())
        }
        Some("eval") => {
            transcribe_cpp::disable_logging();
            eval::parse_args(&args[1..]).and_then(|parsed| eval::run(&parsed, &mut engine::RealEngine::default()))
        }
        Some("isolate") => isolate::run_file(&args[1..]),
        _ => Err("usage: ot-stt-worker serve | eval --model <gguf> --dir <dir> --label <name> [--vocab-file <json>] [--context]".to_string()),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}
