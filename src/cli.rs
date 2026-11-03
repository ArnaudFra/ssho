use anyhow::Result;
use std::path::PathBuf;

#[derive(Debug, Default, PartialEq)]
pub struct Invocation {
    pub config: Option<PathBuf>,
    pub tty: bool,
    pub search: bool,
    pub initial_query: Option<String>,
    pub no_update_check: bool,
    pub command: Command,
}

#[derive(Debug, Default, PartialEq)]
pub enum Command {
    #[default]
    List,
    Help(Option<String>),
    Version,
    Connect {
        host: String,
        remote: Vec<String>,
    },
    Add(String),
    Edit(String),
    Move(String),
    Info {
        host: String,
        pretty: bool,
    },
    Search {
        query: String,
        format: String,
        tags: bool,
        names: bool,
    },
    Completion(String),
    Complete(Vec<String>),
}

pub fn parse(args: &[String]) -> Result<Invocation> {
    let mut result = Invocation::default();
    let mut positional = Vec::new();
    let mut pretty = false;
    let mut tags = false;
    let mut names = false;
    let mut format = "table".to_string();
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        let mode = positional.first().map(String::as_str);
        if matches!(mode, Some("__complete" | "__completeNoDesc")) {
            positional.extend_from_slice(&args[i..]);
            break;
        }
        if arg == "--" {
            positional.extend_from_slice(&args[i + 1..]);
            break;
        }
        let (flag, inline) = arg
            .split_once('=')
            .map_or((arg.as_str(), None), |(a, b)| (a, Some(b)));
        let boolean = || -> Result<bool> {
            match inline {
                None | Some("true") => Ok(true),
                Some("false") => Ok(false),
                _ => anyhow::bail!("invalid boolean for {flag}"),
            }
        };
        match flag {
            "-h" | "--help" => {
                anyhow::ensure!(
                    result.initial_query.is_none() || mode.is_none(),
                    "--query is only valid at the root, without an alias or subcommand"
                );
                result.command = Command::Help(mode.map(str::to_owned));
                return Ok(result);
            }
            "-v" | "--version" => {
                anyhow::ensure!(
                    result.initial_query.is_none() || mode.is_none(),
                    "--query is only valid at the root, without an alias or subcommand"
                );
                result.command = Command::Version;
                return Ok(result);
            }
            "-c" | "--config" => {
                let value = if let Some(value) = inline {
                    value.to_string()
                } else {
                    i += 1;
                    args.get(i)
                        .cloned()
                        .ok_or_else(|| anyhow::anyhow!("{flag} requires a file path"))?
                };
                result.config = Some(value.into());
            }
            "-t" | "--tty" => result.tty = boolean()?,
            "-s" | "--search" => result.search = boolean()?,
            "--query" => {
                anyhow::ensure!(
                    mode.is_none(),
                    "--query is only valid at the root, without an alias or subcommand"
                );
                let value = if let Some(value) = inline {
                    value.to_owned()
                } else {
                    i += 1;
                    args.get(i).cloned().ok_or_else(|| {
                        anyhow::anyhow!(
                            "--query requires search text (use --query '' for an empty search)"
                        )
                    })?
                };
                anyhow::ensure!(
                    !value.chars().any(|c| c.is_control() || matches!(c, '\u{2028}' | '\u{2029}')),
                    "--query requires single-line text without control characters or line separators"
                );
                result.initial_query = Some(value);
            }
            "--no-update-check" => result.no_update_check = boolean()?,
            "--pretty" if mode == Some("info") => pretty = boolean()?,
            "--tags" if mode == Some("search") => tags = boolean()?,
            "--names" if mode == Some("search") => names = boolean()?,
            "-f" | "--format" if mode == Some("search") => {
                format = if let Some(v) = inline {
                    v.into()
                } else {
                    i += 1;
                    args.get(i)
                        .cloned()
                        .ok_or_else(|| anyhow::anyhow!("{flag} requires a format"))?
                };
            }
            _ if arg.starts_with("-c") && arg.len() > 2 => result.config = Some(arg[2..].into()),
            _ if arg.starts_with('-') => anyhow::bail!("unknown option: {arg}"),
            _ => {
                let is_subcommand = matches!(
                    mode,
                    Some("add" | "edit" | "move" | "search" | "info" | "completion" | "help")
                );
                if !positional.is_empty() && !is_subcommand {
                    positional.extend_from_slice(&args[i..]);
                    break;
                }
                positional.push(arg.clone());
            }
        }
        i += 1;
    }
    let Some(first) = positional.first() else {
        return Ok(result);
    };
    anyhow::ensure!(
        result.initial_query.is_none(),
        "--query is only valid at the root, without an alias or subcommand"
    );
    let tail = &positional[1..];
    let exactly_one = || -> Result<String> {
        anyhow::ensure!(tail.len() == 1, "{first} requires exactly one argument");
        Ok(tail[0].clone())
    };
    result.command = match first.as_str() {
        "help" => {
            anyhow::ensure!(tail.len() <= 1, "help accepts at most one command");
            Command::Help(tail.first().cloned())
        }
        "add" => {
            anyhow::ensure!(tail.len() <= 1, "add accepts at most one hostname");
            Command::Add(tail.first().cloned().unwrap_or_default())
        }
        "edit" => Command::Edit(exactly_one()?),
        "move" => Command::Move(exactly_one()?),
        "info" => Command::Info {
            host: exactly_one()?,
            pretty,
        },
        "search" => {
            anyhow::ensure!(tail.len() <= 1, "search accepts at most one query");
            Command::Search {
                query: tail.first().cloned().unwrap_or_default(),
                format,
                tags,
                names,
            }
        }
        "completion" => {
            let shell = exactly_one()?;
            anyhow::ensure!(
                ["bash", "zsh", "fish", "powershell"].contains(&shell.as_str()),
                "unsupported shell: {shell}"
            );
            Command::Completion(shell)
        }
        "__complete" | "__completeNoDesc" => Command::Complete(tail.to_vec()),
        _ => Command::Connect {
            host: first.clone(),
            remote: tail.to_vec(),
        },
    };
    Ok(result)
}

pub fn info_response(path: &std::path::Path, name: &str) -> (serde_json::Value, i32) {
    info_from_hosts(path, name, crate::config::load_hosts(path))
}
fn info_from_hosts(
    path: &std::path::Path,
    name: &str,
    loaded: Result<Vec<crate::types::Host>>,
) -> (serde_json::Value, i32) {
    use serde_json::{json, Value};
    let failure = |code: &str, message: String, exit| {
        (
            json!({"schema":"sshm.info.v1","ok":false,"hostname":name,"result":null,"error":{"code":code,"message":message,"details":null}}),
            exit,
        )
    };
    let hosts = match loaded {
        Ok(hosts) => hosts,
        Err(error) => return failure("CONFIG_ERROR", format!("{error:#}"), 1),
    };
    let Some(host) = hosts.into_iter().find(|host| host.name == name) else {
        return failure("NOT_FOUND", format!("host '{name}' not found"), 2);
    };
    let resolution = crate::runtime::effective::resolve(path, name);
    let (effective, resolution_status, exit) = match resolution {
        Ok(value) => (json!(value), json!({"status":"resolved", "error":null}), 0),
        Err(error) => (
            Value::Null,
            json!({"status":"error", "error":format!("{error:#}")}),
            1,
        ),
    };
    let optional = |s: &str| {
        if s.trim().is_empty() {
            Value::Null
        } else {
            json!(s.trim())
        }
    };
    let port = if host.port.trim().is_empty() {
        Value::Null
    } else {
        match host.port.trim().parse::<i64>() {
            Ok(p) => json!(p),
            Err(_) => {
                return failure(
                    "CONFIG_ERROR",
                    "invalid port in host configuration".into(),
                    1,
                )
            }
        }
    };
    let tags = if host.all_tags().is_empty() {
        Value::Null
    } else {
        json!(public_tags(&host))
    };
    let declared = json!({"hostname":optional(&host.hostname),"user":optional(&host.user),"port":port,
        "identity_file":optional(&host.identity),"extra_identities":host.extra_identities,
        "proxy_jump":optional(&host.proxy_jump),"proxy_command":optional(&host.proxy_command),
        "remote_command":optional(&host.remote_command),"request_tty":optional(&host.request_tty)});
    let target = if effective.is_null() {
        json!({"host":name,"hostname":optional(&host.hostname),"user":optional(&host.user),"port":port})
    } else {
        json!({"host":name,"hostname":effective["hostname"],"user":effective["user"],"port":effective["port"].as_str().and_then(|p| p.parse::<u16>().ok())})
    };
    let mut response = json!({"schema":"sshm.info.v1","ok":exit == 0,"hostname":name,"error":if exit == 0 { Value::Null } else { json!({"code":"RESOLUTION_ERROR","message":resolution_status["error"],"details":null}) },"result":{
        "canonical_name":host.name,"target":target,
        "effective":effective,"resolution":resolution_status,"declared":declared,
        "identity_file":optional(&host.identity),"proxy_jump":optional(&host.proxy_jump),"proxy_command":optional(&host.proxy_command),"options":optional(&host.options),"tags":tags,
        "remote_command":optional(&host.remote_command),"request_tty":optional(&host.request_tty),"source":{"file":host.source_file,"line":host.line_number}
    }});
    add_tag_provenance(&mut response["result"], &host);
    (response, exit)
}
pub fn filter_search(
    hosts: &[crate::types::Host],
    query: &str,
    tags: bool,
    names: bool,
) -> Vec<crate::types::Host> {
    let query = query.to_lowercase();
    hosts
        .iter()
        .filter(|h| !h.hidden())
        .filter(|h| {
            query.is_empty()
                || (!tags
                    && (h.name.to_lowercase().contains(&query)
                        || (!names && h.hostname.to_lowercase().contains(&query))))
                || (!names
                    && h.all_tags()
                        .iter()
                        .any(|t| t.to_lowercase().contains(&query)))
        })
        .cloned()
        .collect()
}
pub fn render_search(hosts: &[crate::types::Host], format: &str) -> String {
    if format == "simple" {
        return hosts.iter().map(|h| format!("{}\n", h.name)).collect();
    }
    if format == "json" {
        let entries: Vec<_> = hosts.iter().map(|h| {
            let mut entry = serde_json::json!({"name":h.name,"hostname":h.hostname,"user":h.user,"port":h.port,"identity":h.identity,"proxy_jump":h.proxy_jump,"proxy_command":h.proxy_command,"options":h.options,"tags":public_tags(h)});
            add_tag_provenance(&mut entry, h);
            entry
        }).collect();
        return format!(
            "{}\n",
            serde_json::to_string_pretty(&entries).expect("host values serialize")
        );
    }
    let widths = [
        hosts.iter().map(|h| h.name.len()).max().unwrap_or(0).max(4) + 2,
        hosts
            .iter()
            .map(|h| h.hostname.len())
            .max()
            .unwrap_or(0)
            .max(8)
            + 2,
        hosts.iter().map(|h| h.user.len()).max().unwrap_or(0).max(4) + 2,
        hosts
            .iter()
            .map(|h| table_tags(h).len())
            .max()
            .unwrap_or(0)
            .max(4)
            + 2,
    ];
    let row = |values: [&str; 4]| {
        format!(
            "{:<w0$} {:<w1$} {:<w2$} {:<w3$}\n",
            values[0],
            values[1],
            values[2],
            values[3],
            w0 = widths[0],
            w1 = widths[1],
            w2 = widths[2],
            w3 = widths[3]
        )
    };
    let mut output = row(["Name", "Hostname", "User", "Tags"]);
    output.push_str(
        &widths
            .iter()
            .map(|n| "-".repeat(*n))
            .collect::<Vec<_>>()
            .join(" "),
    );
    output.push('\n');
    for h in hosts {
        let tags = table_tags(h);
        output.push_str(&row([
            &h.name,
            &h.hostname,
            if h.user.is_empty() { "-" } else { &h.user },
            if tags.is_empty() { "-" } else { &tags },
        ]));
    }
    output.push_str(&format!("\nFound {} host(s)\n", hosts.len()));
    output
}

fn public_tags(host: &crate::types::Host) -> Vec<String> {
    if host.file_tags.is_empty() {
        host.tags.clone()
    } else {
        host.all_tags()
    }
}
fn table_tags(host: &crate::types::Host) -> String {
    // Preserve the established table bytes for configurations without FileTags.
    if host.file_tags.is_empty() {
        host.tags.join(", ")
    } else {
        host.display_tags()
    }
}
fn add_tag_provenance(value: &mut serde_json::Value, host: &crate::types::Host) {
    if !host.file_tags.is_empty() {
        value["host_tags"] = serde_json::json!(host.tags);
        value["file_tags"] = serde_json::json!(host.file_tags);
    }
}
fn emit_warnings(warnings: &[crate::config::ConfigWarning]) {
    for warning in warnings {
        eprintln!("Warning: {warning}");
    }
}

pub fn run(args: &[String]) -> Result<i32> {
    use crate::{
        config,
        storage::AppPaths,
        ui::{self, InitialView},
    };
    let invocation = parse(args)?;
    match &invocation.command {
        Command::Help(command) => {
            print!("{}", help(command.as_deref()));
            return Ok(0);
        }
        Command::Version => {
            println!("ssho version {}", env!("CARGO_PKG_VERSION"));
            return Ok(0);
        }
        Command::Completion(shell) => {
            print!("{}", completion_script(shell)?);
            return Ok(0);
        }
        _ => {}
    }
    let paths = AppPaths::discover()?;
    let path = invocation.config.as_deref().unwrap_or(&paths.ssh_config);
    match invocation.command {
        Command::Info { host, pretty } => {
            let loaded = config::load_report(path).map(|report| {
                emit_warnings(&report.warnings);
                report.hosts
            });
            let (response, code) = info_from_hosts(path, &host, loaded);
            println!(
                "{}",
                if pretty {
                    serde_json::to_string_pretty(&response)?
                } else {
                    serde_json::to_string(&response)?
                }
            );
            Ok(code)
        }
        Command::Search {
            query,
            format,
            tags,
            names,
        } => {
            let report = config::load_report(path)?;
            emit_warnings(&report.warnings);
            let file_tags_json = report.has_file_tags && format == "json";
            let hosts = report.hosts;
            if hosts.is_empty() {
                if file_tags_json {
                    println!("[]");
                } else {
                    println!("No SSH hosts found in your configuration file.");
                }
                return Ok(1);
            }
            let hosts = filter_search(&hosts, &query, tags, names);
            if hosts.is_empty() {
                if file_tags_json {
                    println!("[]");
                } else if query.is_empty() {
                    println!("No hosts found.")
                } else {
                    println!("No hosts found matching '{query}'.")
                }
                return Ok(0);
            }
            print!("{}", render_search(&hosts, &format));
            Ok(0)
        }
        Command::Connect { host, remote } => {
            emit_warnings(&config::load_report(path)?.warnings);
            crate::runtime::connect(&paths, path, &host, invocation.tty, &remote, None)
        }
        Command::Complete(words) => {
            let mut warnings = Vec::new();
            let values = complete_words(&words, path, &mut warnings)?;
            emit_warnings(&warnings);
            for value in values {
                println!("{value}")
            }
            println!(":4");
            Ok(0)
        }
        command => {
            let migrated = paths.initialize()?;
            let initial = match command {
                Command::Add(seed) => InitialView::Add(seed),
                Command::Edit(name) => InitialView::Edit(name),
                Command::Move(name) => InitialView::Move(name),
                _ => match invocation.initial_query {
                    Some(query) => InitialView::ListQuery(query),
                    None => InitialView::List,
                },
            };
            let report = config::load_report(path)?;
            if matches!(initial, InitialView::List) && report.hosts.is_empty() {
                emit_warnings(&report.warnings);
                use std::io::{self, Write};
                println!("No SSH hosts found in your ~/.ssh/config file.");
                print!("Would you like to add a new host now? [y/N]: ");
                io::stdout().flush()?;
                let mut response = String::new();
                io::stdin().read_line(&mut response)?;
                if !response.trim().eq_ignore_ascii_case("y") {
                    println!("No hosts available, exiting.");
                    return Ok(1);
                }
                let code = ui::run(
                    &paths,
                    path,
                    invocation.search,
                    invocation.no_update_check,
                    migrated,
                    InitialView::Add(String::new()),
                )?;
                if code != 0 {
                    return Ok(code);
                }
                if config::load_hosts(path)?.is_empty() {
                    println!("No hosts available, exiting.");
                    return Ok(1);
                }
                ui::run(
                    &paths,
                    path,
                    invocation.search,
                    invocation.no_update_check,
                    migrated,
                    InitialView::List,
                )
            } else {
                ui::run(
                    &paths,
                    path,
                    invocation.search,
                    invocation.no_update_check,
                    migrated,
                    initial,
                )
            }
        }
    }
}

const SUBCOMMANDS: &[&str] = &[
    "add",
    "edit",
    "move",
    "info",
    "search",
    "completion",
    "help",
];
const SHELLS: &[&str] = &["bash", "zsh", "fish", "powershell"];
const FORMATS: &[&str] = &["table", "json", "simple"];

fn complete_words(
    words: &[String],
    default_config: &std::path::Path,
    warnings: &mut Vec<crate::config::ConfigWarning>,
) -> Result<Vec<String>> {
    let prefix = words.last().map(String::as_str).unwrap_or("");
    let prior = words
        .get(..words.len().saturating_sub(1))
        .unwrap_or_default();
    let mut config = default_config.to_path_buf();
    let mut positional = Vec::new();
    let mut value_for = None;
    let mut options = true;
    let mut initial_query = false;
    for word in prior {
        if let Some(flag) = value_for.take() {
            if flag == "config" {
                config = completion_path(word);
            }
            continue;
        }
        if options && word == "--" {
            options = false;
        } else if options && matches!(word.as_str(), "-c" | "--config") {
            value_for = Some("config");
        } else if options && word == "--query" {
            initial_query = true;
            value_for = Some("query");
        } else if options && word.starts_with("--query=") {
            initial_query = true;
        } else if options && config_value(word).is_some() {
            config = completion_path(config_value(word).unwrap().1);
        } else if options
            && positional.first() == Some(&"search")
            && matches!(word.as_str(), "-f" | "--format")
        {
            value_for = Some("format");
        } else if !options || !word.starts_with('-') {
            positional.push(word.as_str());
        }
    }
    if value_for == Some("query") || (options && prefix.starts_with("--query=")) {
        return Ok(Vec::new());
    }
    if value_for == Some("config") {
        return Ok(complete_files(prefix, ""));
    }
    if value_for == Some("format") {
        return Ok(complete_values(FORMATS, prefix, ""));
    }
    let command = positional
        .first()
        .copied()
        .filter(|v| SUBCOMMANDS.contains(v));
    if options {
        if let Some((flag, value)) = config_value(prefix) {
            return Ok(complete_files(value, flag));
        }
        if command == Some("search") {
            if let Some(value) = prefix.strip_prefix("--format=") {
                return Ok(complete_values(FORMATS, value, "--format="));
            }
            if let Some(value) = prefix.strip_prefix("-f=") {
                return Ok(complete_values(FORMATS, value, "-f="));
            }
        }
        if prefix.starts_with('-') {
            let mut flags = vec![
                "-c",
                "--config",
                "-t",
                "--tty",
                "-s",
                "--search",
                "--no-update-check",
                "-h",
                "--help",
                "-v",
                "--version",
            ];
            if positional.is_empty() {
                flags.push("--query");
            }
            match command {
                Some("search") => flags.extend(["-f", "--format", "--names", "--tags"]),
                Some("info") => flags.push("--pretty"),
                _ => {}
            }
            return Ok(complete_values(&flags, prefix, ""));
        }
    }
    if initial_query {
        return Ok(Vec::new());
    }
    if command == Some("completion") && positional.len() == 1 {
        return Ok(complete_values(SHELLS, prefix, ""));
    }
    if command == Some("help") && positional.len() == 1 {
        return Ok(complete_values(SUBCOMMANDS, prefix, ""));
    }
    let mut completions = if positional.is_empty() && options {
        complete_values(SUBCOMMANDS, prefix, "")
    } else {
        Vec::new()
    };
    let hosts_allowed = positional.is_empty()
        || (positional.len() == 1 && matches!(command, Some("edit" | "move" | "info")));
    if hosts_allowed {
        // A missing/unreadable config must not suppress static command candidates.
        let hosts = crate::config::load_report(&config)
            .map(|report| {
                *warnings = report.warnings;
                report.hosts
            })
            .unwrap_or_default();
        for host in hosts {
            if (command == Some("info") || !host.hidden())
                && host.name.to_lowercase().starts_with(&prefix.to_lowercase())
                && !completions.contains(&host.name)
            {
                completions.push(host.name);
            }
        }
    }
    Ok(completions)
}

fn complete_values(values: &[&str], prefix: &str, flag: &str) -> Vec<String> {
    values
        .iter()
        .filter(|v| v.starts_with(prefix))
        .map(|v| format!("{flag}{v}"))
        .collect()
}

// Return the exact option prefix so inline file candidates replace the whole word.
fn config_value(word: &str) -> Option<(&str, &str)> {
    for flag in ["--config=", "-c="] {
        if let Some(value) = word.strip_prefix(flag) {
            return Some((flag, value));
        }
    }
    word.strip_prefix("-c")
        .filter(|v| !v.is_empty())
        .map(|v| ("-c", v))
}

fn completion_path(value: &str) -> PathBuf {
    if let Some(rest) = value.strip_prefix("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home).join(rest);
        }
    }
    PathBuf::from(value)
}

fn complete_files(prefix: &str, flag: &str) -> Vec<String> {
    let split = prefix.rfind(std::path::is_separator).map_or(0, |i| i + 1);
    let (directory, basename) = prefix.split_at(split);
    let path = if directory.is_empty() {
        PathBuf::from(".")
    } else {
        completion_path(directory)
    };
    let Ok(entries) = std::fs::read_dir(path) else {
        return Vec::new();
    };
    let mut candidates = Vec::new();
    for entry in entries.flatten() {
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        if !name.starts_with(basename)
            || (basename.is_empty() && name.starts_with('.'))
            || name.contains(['\n', '\r', '\t'])
        {
            continue;
        }
        let slash = if entry.path().is_dir() { "/" } else { "" };
        candidates.push(format!("{flag}{directory}{name}{slash}"));
    }
    candidates.sort();
    candidates
}

pub fn help(command: Option<&str>) -> String {
    let usage = match command {
        Some("add") => "ssho add [hostname]",
        Some("edit") => "ssho edit <host>",
        Some("move") => "ssho move <host>",
        Some("info") => "ssho info <host> [--pretty]",
        Some("search") => "ssho search [query] [--tags] [--names] [-f table|json|simple]",
        Some("completion") => "ssho completion bash|zsh|fish|powershell",
        _ => "ssho [host] [command...]",
    };
    format!("SSH Organizer - A modern SSH connection manager\n\nssho is a modern SSH manager for your terminal, implemented in Rust.\n\nUsage:\n  {usage}\n\nCommands:\n  add [hostname]       Add a host using an interactive form\n  edit <host>          Edit a host and its aliases\n  move <host>          Move a host to another included configuration\n  search [query]       Search by name, hostname or tags\n  info <host>          Print effective SSH JSON (--pretty to indent)\n  completion <shell>   Generate Bash, Zsh, Fish or PowerShell completion\n  help [command]       Show help\n\nOptions:\n  -c, --config <file>   SSH configuration file (default: ~/.ssh/config)\n  -t, --tty             Force a pseudo-TTY for the remote command\n  -s, --search          Focus search at startup\n      --query <text>     Open TUI with editable initial search (root only)\n      --no-update-check  Disable automatic update checking\n  -h, --help            Show help\n  -v, --version         Show version\n\nSearch options:\n  -f, --format <format>  table, json or simple (default: table)\n      --tags            Search tags only\n      --names           Search names only\n\nExamples:\n  ssho\n  ssho --query 'tag:prod AND user:deploy'\n  ssho --query ''\n  ssho prod-server uptime\n  ssho -t prod-server sudo systemctl restart nginx\n  ssho search --tags work --format json\n\nApplication data: ~/.config/ssho/ (respects XDG_CONFIG_HOME).\n")
}

pub fn completion_script(shell: &str) -> Result<&'static str> {
    Ok(match shell {
        "bash" => {
            r#"_ssho_unquote() {
    local raw=$1 quote= value= char next i
    for ((i=0; i<${#raw}; i++)); do
        char=${raw:i:1}
        if [[ "$char" == '\' && "$quote" != "'" ]]; then
            next=${raw:i+1:1}
            if [[ -n "$next" && ( "$quote" != '"' || "$next" == '\' || "$next" == '$' || "$next" == '`' || "$next" == '"' ) ]]; then
                value+=$next
                ((i++))
            else
                value+=$char
            fi
        elif [[ -z "$quote" && ( "$char" == "'" || "$char" == '"' ) ]]; then
            quote=$char
        elif [[ "$char" == "$quote" ]]; then
            quote=
        else
            value+=$char
        fi
    done
    REPLY=$value
}
_ssho() {
    local line word REPLY previous= i=0
    local -a args=()
    for word in "${COMP_WORDS[@]:1:COMP_CWORD}"; do
        i=$((i + 1))
        if [[ "$word" == '=' && ( "$previous" == '--config' || "$previous" == '-c' || "$previous" == '--format' || "$previous" == '-f' || "$previous" == '--query' ) ]]; then
            if ((i == COMP_CWORD)); then args+=(''); fi
            continue
        fi
        _ssho_unquote "$word"
        args+=("$REPLY")
        previous=$REPLY
    done
    COMPREPLY=()
    while IFS= read -r line; do
        [[ "$line" == :* ]] || COMPREPLY+=("$line")
    done < <("${COMP_WORDS[0]}" __complete "${args[@]}" 2>/dev/null)
}
complete -o filenames -F _ssho ssho
"#
        }
        "zsh" => {
            r#"#compdef ssho
_ssho() {
    local -a candidates
    local line
    while IFS= read -r line; do
        [[ "$line" == :* ]] || candidates+=("$line")
    done < <(ssho __complete "${words[@]:1:$((CURRENT - 1))}" 2>/dev/null)
    compadd -f -- "${candidates[@]}"
}
compdef _ssho ssho
"#
        }
        "fish" => {
            r#"function __ssho_complete
    set -l current (commandline -ct | string unescape)
    ssho __complete (commandline -opc)[2..-1] "$current" 2>/dev/null | string match -rv '^:'
end
complete -c ssho -f -a '(__ssho_complete)'
"#
        }
        "powershell" => {
            r#"Register-ArgumentCompleter -Native -CommandName ssho -ScriptBlock {
    param($wordToComplete, $commandAst, $cursorPosition)
    $words = @($commandAst.CommandElements | Select-Object -Skip 1 | Where-Object {
        $_.Extent.StartOffset -lt $cursorPosition
    } | ForEach-Object {
        if ($_ -is [System.Management.Automation.Language.StringConstantExpressionAst]) { $_.Value }
        else { $_.Extent.Text }
    })
    if (-not $wordToComplete) { $words += '' }
    & ssho __complete @words 2>$null | Where-Object { $_ -notmatch '^:' } | ForEach-Object {
        $quoted = "'" + $_.Replace("'", "''") + "'"
        [System.Management.Automation.CompletionResult]::new($quoted, $_, 'ParameterValue', $_)
    }
}
"#
        }
        _ => anyhow::bail!("unsupported shell: {shell}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn args(s: &[&str]) -> Vec<String> {
        s.iter().map(|v| v.to_string()).collect()
    }
    #[test]
    fn initial_query_accepts_empty_unicode_inline_and_legacy_search_boolean() {
        for words in [args(&["--query", ""]), args(&["--query="])] {
            let invocation = parse(&words).expect("empty query opens editable TUI");
            assert_eq!(invocation.command, Command::List);
            assert_eq!(invocation.initial_query.as_deref(), Some(""));
        }
        for (words, expected) in [
            (
                args(&["--query", "tag:prod AND café 東京", "--search=false"]),
                "tag:prod AND café 東京",
            ),
            (
                args(&["-c", "custom", "--query=tag:prod AND café 東京"]),
                "tag:prod AND café 東京",
            ),
            (args(&["--query", "--literal"]), "--literal"),
        ] {
            let invocation = parse(&words).unwrap();
            assert_eq!(invocation.command, Command::List);
            assert_eq!(invocation.initial_query.as_deref(), Some(expected));
        }
        assert!(!parse(&args(&["--search=false"])).unwrap().search);
        assert!(parse(&args(&["--search", "prod"])).is_ok());
    }

    #[test]
    fn initial_query_rejects_missing_controls_and_non_root_combinations() {
        assert!(parse(&args(&["--query"]))
            .unwrap_err()
            .to_string()
            .contains("requires"));
        for value in [
            "a\nb", "a\rb", "a\tb", "a\0b", "a\u{1b}b", "a\u{7f}b", "a\u{85}b",
        ] {
            let error = parse(&args(&["--query", value])).unwrap_err().to_string();
            assert!(error.contains("control"), "{value:?}: {error}");
        }
        for words in [
            args(&["--query", "prod", "prod"]),
            args(&["prod", "--query", "prod"]),
            args(&["--query=prod", "search", "prod"]),
            args(&["search", "--query=prod"]),
            args(&["add", "--query=prod"]),
            args(&["--query=prod", "edit", "prod"]),
            args(&["--query=prod", "move", "prod"]),
            args(&["--query=prod", "info", "prod"]),
            args(&["--query=prod", "completion", "bash"]),
            args(&["--query=prod", "help", "search"]),
        ] {
            let error = parse(&words).unwrap_err().to_string();
            assert!(error.contains("root"), "{words:?}: {error}");
        }
    }

    #[test]
    fn initial_query_remote_argv_stays_literal_after_command_starts() {
        for words in [
            args(&["prod", "echo", "--query", "café\ntext"]),
            args(&["prod", "--", "echo", "--query", "café\ntext"]),
        ] {
            assert_eq!(
                parse(&words).unwrap().command,
                Command::Connect {
                    host: "prod".into(),
                    remote: args(&["echo", "--query", "café\ntext"])
                }
            );
        }
    }

    #[test]
    fn direct_command_keeps_remote_flags_and_arguments() {
        let p = parse(&args(&[
            "-t",
            "prod",
            "ls",
            "-la",
            "/a b",
            "--config",
            "remote-file",
        ]))
        .unwrap();
        assert!(p.tty);
        assert_eq!(p.config, None);
        assert_eq!(
            p.command,
            Command::Connect {
                host: "prod".into(),
                remote: args(&["ls", "-la", "/a b", "--config", "remote-file"])
            }
        );
    }
    #[test]
    fn config_after_host_and_escaped_remote_command() {
        let p = parse(&args(&[
            "prod",
            "-c",
            "/tmp/config",
            "--",
            "echo",
            "-n",
            "hi",
        ]))
        .unwrap();
        assert_eq!(p.config, Some(PathBuf::from("/tmp/config")));
        assert_eq!(
            p.command,
            Command::Connect {
                host: "prod".into(),
                remote: args(&["echo", "-n", "hi"])
            }
        );
    }
    #[test]
    fn search_options_and_global_flags_can_be_interspersed() {
        let p = parse(&args(&[
            "search", "--tags", "work", "-f", "json", "-c", "test",
        ]))
        .unwrap();
        assert_eq!(p.config, Some(PathBuf::from("test")));
        assert_eq!(
            p.command,
            Command::Search {
                query: "work".into(),
                format: "json".into(),
                tags: true,
                names: false
            }
        );
        assert!(parse(&args(&["search", "one", "two"])).is_err());
        assert!(parse(&args(&["info"])).is_err());
        assert!(parse(&args(&["edit", "one", "two"])).is_err());
    }
    #[test]
    fn info_pretty_and_help_do_not_require_host() {
        assert_eq!(
            parse(&args(&["info", "server", "--pretty"]))
                .unwrap()
                .command,
            Command::Info {
                host: "server".into(),
                pretty: true
            }
        );
        assert_eq!(
            parse(&args(&["info", "--help"])).unwrap().command,
            Command::Help(Some("info".into()))
        );
        assert_eq!(
            parse(&args(&["--version"])).unwrap().command,
            Command::Version
        );
    }
    #[test]
    fn info_preserves_schema_nulls_and_not_found_exit_code() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config");
        std::fs::write(&path, "Host db\n HostName db.example.org\n Port 2222\n").unwrap();
        let (value, code) = info_response(&path, "db");
        assert_eq!(code, 0);
        assert_eq!(value["schema"], "sshm.info.v1");
        assert_eq!(value["ok"], true);
        assert_eq!(value["result"]["target"]["port"], 2222);
        assert!(value["result"]["identity_file"].is_null());
        assert!(value["result"]["tags"].is_null());
        let (value, code) = info_response(&path, "missing");
        assert_eq!(code, 2);
        assert_eq!(value["error"]["code"], "NOT_FOUND");
        assert_eq!(value["error"]["message"], "host 'missing' not found");
        let (value, code) = info_response(&dir.path().join("absent"), "missing");
        assert!(code != 0);
        assert_eq!(value["ok"], false);
    }
    #[test]
    fn search_cli_filters_fields_and_escapes_json() {
        let hosts = vec![
            crate::types::Host {
                name: "prod".into(),
                hostname: "db.example.org".into(),
                user: "deploy".into(),
                tags: vec!["work".into()],
                ..Default::default()
            },
            crate::types::Host {
                name: "hidden".into(),
                tags: vec!["hidden".into()],
                ..Default::default()
            },
        ];
        assert_eq!(filter_search(&hosts, "WORK", true, false).len(), 1);
        assert_eq!(filter_search(&hosts, "example", false, true).len(), 0);
        assert_eq!(filter_search(&hosts, "deploy", false, false).len(), 0);
        assert_eq!(filter_search(&hosts, "", false, false).len(), 1);
        let json: serde_json::Value =
            serde_json::from_str(&render_search(&hosts[..1], "json")).unwrap();
        assert_eq!(json[0]["name"], "prod");
        assert_eq!(json[0]["port"], "");
        assert_eq!(json[0]["tags"], serde_json::json!(["work"]));
        assert_eq!(render_search(&hosts[..1], "simple"), "prod\n");
    }
}
