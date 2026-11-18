//! Puts a transcribed meeting through the steps the app takes after the text is written
//! out: words put right, who said what checked, names found and minutes written. Writes
//! each result next to the transcript and prints counts, never what was said.
//!
//! `cargo run --release --example meeting_eval -- --in <transcript.txt> --out <dir>`
//! Groq with `GROQ_API_KEY` by default; `GLM=1` uses the z.ai endpoint with `GLM_API_KEY`.
//! `MODEL` overrides the model.

use opentypeless_lib::minutes::{self, Said};
use opentypeless_lib::storage::AppConfig;

const LABEL: &str = "说话人";

fn arg(name: &str) -> String {
    let args: Vec<String> = std::env::args().collect();
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1).cloned())
        .unwrap_or_else(|| panic!("missing {name}"))
}

/// The speaker number and time of a heading line, the number 0 for a voice too short to tell.
fn heading(line: &str) -> Option<(u32, String)> {
    let rest = line.strip_prefix(LABEL)?;
    let (number, time) = rest
        .trim_start()
        .split_once("  ")
        .unwrap_or(("", rest.trim()));
    let time = time.trim();
    let timed = !time.is_empty() && time.chars().all(|c| c.is_ascii_digit() || c == ':');
    if !timed {
        return None;
    }
    Some((number.trim().parse().unwrap_or(0), time.to_string()))
}

/// The meeting as paragraphs: speaker, time and words.
fn paragraphs(text: &str) -> Vec<(u32, String, String)> {
    let mut out: Vec<(u32, String, String)> = Vec::new();
    for line in text.lines() {
        if let Some((speaker, time)) = heading(line) {
            out.push((speaker, time, String::new()));
        } else if !line.trim().is_empty() {
            match out.last_mut() {
                Some(last) if last.2.is_empty() => last.2 = line.trim().to_string(),
                Some(last) => {
                    last.2.push(' ');
                    last.2.push_str(line.trim());
                }
                None => out.push((0, "00:00".into(), line.trim().to_string())),
            }
        }
    }
    out
}

fn written(paragraphs: &[(u32, String, String)], names: &[(u32, String)]) -> String {
    paragraphs
        .iter()
        .map(|(speaker, time, text)| {
            let who = names
                .iter()
                .find(|(number, _)| number == speaker)
                .map(|(_, name)| name.clone())
                .unwrap_or_else(|| format!("{LABEL} {speaker}"));
            format!("{who}  {time}\n{text}")
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

#[tokio::main]
async fn main() {
    let input = arg("--in");
    let out = std::path::PathBuf::from(arg("--out"));
    let glm = std::env::var("GLM").is_ok();
    let config = AppConfig {
        llm_provider: if glm { "zhipu" } else { "groq" }.into(),
        llm_base_url: if glm {
            "https://api.z.ai/api/anthropic/v1"
        } else {
            "https://api.groq.com/openai/v1"
        }
        .into(),
        llm_api_format: if glm {
            "anthropic_messages"
        } else {
            "chat_completions"
        }
        .into(),
        llm_model: std::env::var("MODEL").unwrap_or_else(|_| {
            if glm {
                "glm-5.3-flash"
            } else {
                "qwen/qwen3.8-27b"
            }
            .into()
        }),
        ..Default::default()
    };
    let key = std::env::var(if glm { "GLM_API_KEY" } else { "GROQ_API_KEY" }).expect("API key");
    let client = reqwest::Client::new();
    opentypeless_lib::applog::init(&out);
    let raw = std::fs::read_to_string(&input).unwrap();
    let mut paras = paragraphs(&raw);
    println!(
        "{} paragraphs, {} characters",
        paras.len(),
        raw.chars().count()
    );

    let skip = std::env::var("FROM_CHECKED").is_ok();
    if skip {
        paras = paragraphs(&std::fs::read_to_string(out.join("speech-checked.txt")).unwrap());
    } else {
        let started = std::time::Instant::now();
        let pieces: Vec<String> = paras.iter().map(|p| p.2.clone()).collect();
        match minutes::polish(&client, &config, &key, pieces, &[], "", |_, _| {}).await {
            Ok((better, terms)) => {
                for (para, line) in paras.iter_mut().zip(better) {
                    let line = line
                        .replace('¶', " ")
                        .split_whitespace()
                        .collect::<Vec<_>>()
                        .join(" ");
                    if !line.is_empty() {
                        para.2 = line;
                    }
                }
                println!(
                    "words put right in {:.0}s, {} terms unsure",
                    started.elapsed().as_secs_f32(),
                    terms.len()
                );
            }
            Err(error) => println!(
                "words could not be put right: {}",
                error.chars().take(120).collect::<String>()
            ),
        }
        std::fs::write(out.join("speech-polished.txt"), written(&paras, &[])).unwrap();
        if std::env::var("POLISH_ONLY").is_ok() {
            return;
        }

        let started = std::time::Instant::now();
        let said: Vec<Said> = paras
            .iter()
            .map(|p| Said {
                speaker: p.0,
                text: p.2.clone(),
            })
            .collect();
        match minutes::check_speakers(&client, &config, &key, said, |_, _| {}).await {
            Ok(checked) if checked.len() == paras.len() => {
                let moved = paras
                    .iter()
                    .zip(&checked)
                    .filter(|(p, c)| p.0 != c.speaker)
                    .count();
                for (para, piece) in paras.iter_mut().zip(checked) {
                    para.0 = piece.speaker;
                    para.2 = piece.text;
                }
                println!(
                    "speakers checked in {:.0}s, {moved} paragraphs moved",
                    started.elapsed().as_secs_f32()
                );
            }
            Ok(checked) => println!(
                "speaker check came back with {} pieces for {}; not used",
                checked.len(),
                paras.len()
            ),
            Err(error) => println!(
                "speakers could not be checked: {}",
                error.chars().take(120).collect::<String>()
            ),
        }
        std::fs::write(out.join("speech-checked.txt"), written(&paras, &[])).unwrap();
    }

    let said: Vec<Said> = paras
        .iter()
        .map(|p| Said {
            speaker: p.0,
            text: p.2.clone(),
        })
        .collect();
    let found = minutes::find_names(&client, &config, &key, &said)
        .await
        .unwrap_or_else(|error| {
            println!(
                "names could not be found: {}",
                error.chars().take(120).collect::<String>()
            );
            Vec::new()
        });
    println!("{} names found", found.len());
    std::fs::write(
        out.join("speech-names.json"),
        serde_json::to_string_pretty(&found).unwrap(),
    )
    .unwrap();
    let names: Vec<(u32, String)> = found.iter().map(|f| (f.speaker, f.name.clone())).collect();

    let template = "# 会议纪要\n\n## 议题\n\n## 结论\n\n## 讨论要点\n\n## 待办事项\n每条写明负责人和期限\n\n## 未决问题";
    let transcript = written(&paras, &names);
    // CAREFUL=0 or 1 overrides what the model's size decides.
    let careful = std::env::var("CAREFUL")
        .map(|v| v == "1")
        .unwrap_or(!minutes::strong_model(&config.llm_model));
    println!("careful: {careful}");
    let started = std::time::Instant::now();
    match minutes::minutes_with(
        &client,
        &config,
        &key,
        &transcript,
        template,
        "",
        &[],
        careful,
        std::env::var("THINK").is_ok(),
        minutes::Form {
            full: std::env::var("FULL").is_ok(),
            language: std::env::var("LANG_OUT").unwrap_or_default(),
            ..Default::default()
        },
    )
    .await
    {
        Ok(written) => {
            println!(
                "minutes of {} characters in {:.0}s, {} marked unsupported",
                written.chars().count(),
                started.elapsed().as_secs_f32(),
                written.matches('⚠').count()
            );
            std::fs::write(out.join("speech-minutes.md"), written).unwrap();
        }
        Err(error) => println!(
            "minutes failed: {}",
            error.chars().take(200).collect::<String>()
        ),
    }
}
