use anyhow::Result;
use std::path::PathBuf;

#[derive(Debug, Default, PartialEq)]
pub struct Invocation {
    pub config: Option<PathBuf>,
    pub tty: bool,
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
                result.command = Command::Help(mode.map(str::to_owned));
                return Ok(result);
            }
            "-v" | "--version" => {
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
                let is_subcommand = matches!(mode, Some("search" | "info" | "completion" | "help"));
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
    use serde_json::{json, Value};
    let failure = |code: &str, message: String, exit| {
        (
            json!({"schema":"sshm.info.v1","ok":false,"hostname":name,"result":null,"error":{"code":code,"message":message,"details":null}}),
            exit,
        )
    };
    let host = match crate::config::find_host(path, name) {
        Ok(h) => h,
        Err(error) => {
            let mut message = format!("{error:#}");
            let missing = message.to_lowercase().contains("not found");
            if missing {
                message = format!("host '{name}' not found");
            }
            return failure(
                if missing { "NOT_FOUND" } else { "CONFIG_ERROR" },
                message,
                if missing { 2 } else { 1 },
            );
        }
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
    let tags = if host.tags.is_empty() {
        Value::Null
    } else {
        json!(host.tags)
    };
    (
        json!({"schema":"sshm.info.v1","ok":true,"hostname":name,"error":null,"result":{
            "canonical_name":host.name,"target":{"host":name,"hostname":optional(&host.hostname),"user":optional(&host.user),"port":port},
            "identity_file":optional(&host.identity),"proxy_jump":optional(&host.proxy_jump),"proxy_command":optional(&host.proxy_command),"options":optional(&host.options),"tags":tags,
            "remote_command":optional(&host.remote_command),"request_tty":optional(&host.request_tty),"source":{"file":host.source_file,"line":host.line_number}
        }}),
        0,
    )
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
                || (!names && h.tags.iter().any(|t| t.to_lowercase().contains(&query)))
        })
        .cloned()
        .collect()
}
pub fn render_search(hosts: &[crate::types::Host], format: &str) -> String {
    if format == "simple" {
        return hosts.iter().map(|h| format!("{}\n", h.name)).collect();
    }
    if format == "json" {
        let entries:Vec<_>=hosts.iter().map(|h|serde_json::json!({"name":h.name,"hostname":h.hostname,"user":h.user,"port":h.port,"identity":h.identity,"proxy_jump":h.proxy_jump,"proxy_command":h.proxy_command,"options":h.options,"tags":h.tags})).collect();
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
            .map(|h| h.tags.join(", ").len())
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
        let tags = h.tags.join(", ");
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

pub fn run(args: &[String]) -> Result<i32> {
    use crate::{config, storage::AppPaths};
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
            let (response, code) = info_response(path, &host);
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
            let hosts = config::load_hosts(path)?;
            if hosts.is_empty() {
                println!("No SSH hosts found in your configuration file.");
                return Ok(1);
            }
            let hosts = filter_search(&hosts, &query, tags, names);
            if hosts.is_empty() {
                if query.is_empty() {
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
            crate::runtime::connect(&paths, path, &host, invocation.tty, &remote, None)
        }
        Command::Complete(words) => {
            for value in complete_words(&words, path)? {
                println!("{value}")
            }
            println!(":4");
            Ok(0)
        }
        _ => {
            // No interface yet: print the hosts like `search` without a query.
            let hosts = filter_search(&config::load_hosts(path)?, "", false, false);
            if hosts.is_empty() {
                println!("No SSH hosts found in your ~/.ssh/config file.");
                return Ok(1);
            }
            print!("{}", render_search(&hosts, "table"));
            Ok(0)
        }
    }
}

fn complete_words(words: &[String], default_config: &std::path::Path) -> Result<Vec<String>> {
    let prefix = words
        .last()
        .map(String::as_str)
        .unwrap_or("")
        .to_lowercase();
    let prior = if words.is_empty() {
        &[][..]
    } else {
        &words[..words.len() - 1]
    };
    if prior.last().is_some_and(|v| v == "-c" || v == "--config") {
        return Ok(vec![]);
    }
    // Completion consumes partial command lines, so required positionals may still be absent.
    let mut config = default_config.to_path_buf();
    let mut positional = Vec::new();
    let mut i = 0;
    while i < prior.len() {
        let word = &prior[i];
        if word == "-c" || word == "--config" {
            i += 1;
            if let Some(value) = prior.get(i) {
                config = PathBuf::from(value)
            }
        } else if let Some(value) = word.strip_prefix("--config=") {
            config = PathBuf::from(value)
        } else if word.starts_with("-c") && word.len() > 2 {
            config = PathBuf::from(&word[2..])
        } else if !word.starts_with('-') {
            positional.push(word.as_str())
        }
        i += 1;
    }
    let target_command = positional
        .first()
        .copied()
        .filter(|w| ["info", "search", "completion"].contains(w));
    let hosts_allowed =
        positional.is_empty() || (positional.len() == 1 && matches!(target_command, Some("info")));
    let mut completions = Vec::new();
    if prefix.starts_with('-') {
        completions.extend(
            ["--config", "--tty", "--help", "--version"]
                .iter()
                .filter(|s| s.starts_with(&prefix))
                .map(|s| s.to_string()),
        );
    } else if target_command == Some("completion") {
        completions.extend(
            ["bash", "zsh", "fish", "powershell"]
                .iter()
                .filter(|s| s.starts_with(&prefix))
                .map(|s| s.to_string()),
        );
    } else if hosts_allowed {
        for host in crate::config::load_hosts(&config)? {
            if (target_command == Some("info") || !host.hidden())
                && host.name.to_lowercase().starts_with(&prefix)
                && !completions.contains(&host.name)
            {
                completions.push(host.name)
            }
        }
    }
    Ok(completions)
}

pub fn help(command: Option<&str>) -> String {
    let usage = match command {
        Some("info") => "ssho info <host> [--pretty]",
        Some("search") => "ssho search [query] [--tags] [--names] [-f table|json|simple]",
        Some("completion") => "ssho completion bash|zsh|fish|powershell",
        _ => "ssho [host] [command...]",
    };
    format!("SSH Organizer - A modern SSH connection manager\n\nssho is a modern SSH manager for your terminal, implemented in Rust.\n\nUsage:\n  {usage}\n\nCommands:\n  search [query]       Search by name, hostname or tags\n  info <host>          Print machine-readable JSON (--pretty to indent)\n  completion <shell>   Generate Bash, Zsh, Fish or PowerShell completion\n  help [command]       Show help\n\nOptions:\n  -c, --config <file>   SSH configuration file (default: ~/.ssh/config)\n  -t, --tty             Force a pseudo-TTY for the remote command\n  -h, --help            Show help\n  -v, --version         Show version\n\nSearch options:\n  -f, --format <format>  table, json or simple (default: table)\n      --tags            Search tags only\n      --names           Search names only\n\nExamples:\n  ssho\n  ssho prod-server uptime\n  ssho -t prod-server sudo systemctl restart nginx\n  ssho search --tags work --format json\n\nApplication data: ~/.config/ssho/ (respects XDG_CONFIG_HOME).\n")
}

pub fn completion_script(shell: &str) -> Result<&'static str> {
    Ok(match shell {
        "bash" => {
            r#"_ssho() {
    local line
    COMPREPLY=()
    while IFS= read -r line; do
        [[ "$line" == :* ]] || COMPREPLY+=("$line")
    done < <(ssho __complete "${COMP_WORDS[@]:1:COMP_CWORD}" 2>/dev/null)
}
complete -o nospace -F _ssho ssho
"#
        }
        "zsh" => {
            r#"#compdef ssho
_ssho() {
    local -a candidates
    local line
    while IFS= read -r line; do
        [[ "$line" == :* ]] || candidates+=("$line")
    done < <(ssho __complete "${words[@]:1}" 2>/dev/null)
    _describe 'SSH hosts' candidates
}
compdef _ssho ssho
"#
        }
        "fish" => {
            r#"function __ssho_complete
    ssho __complete (commandline -opc)[2..-1] (commandline -ct) 2>/dev/null | string match -rv '^:'
end
complete -c ssho -f -a '(__ssho_complete)'
"#
        }
        "powershell" => {
            r#"Register-ArgumentCompleter -Native -CommandName ssho -ScriptBlock {
    param($wordToComplete, $commandAst, $cursorPosition)
    $words = @($commandAst.CommandElements | Select-Object -Skip 1 | ForEach-Object { $_.ToString().Trim('"', "'") })
    if (-not $wordToComplete) { $words += '' }
    & ssho __complete @words 2>$null | Where-Object { $_ -notmatch '^:' } | ForEach-Object {
        [System.Management.Automation.CompletionResult]::new($_, $_, 'ParameterValue', $_)
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
