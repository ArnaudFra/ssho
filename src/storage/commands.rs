//! Command catalogue: personal named commands with simple typed
//! parameters. A template is not a script: its placeholders are replaced by
//! values that were asked explicitly and inserted as one quoted shell word.
//!
//! Placeholders: `{name}` (text), `{name=default}`, `{name:number}`,
//! `{name:number=10}`, `{name:choice=a|b|c}`; `{{` and `}}` are literal braces.
use super::*;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    /// One host or a batch.
    #[default]
    Any,
    /// Meant for the selected host only.
    Host,
    /// Meant for the marked targets of a batch.
    Batch,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SavedCommand {
    pub description: String,
    pub scope: Scope,
    pub template: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Kind {
    Text,
    Number,
    Choice(Vec<String>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Parameter {
    pub name: String,
    pub kind: Kind,
    pub default: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Part {
    Text(String),
    Value(String),
}

/// Why a template or a value was refused; the interface words it in its language.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TemplateError {
    Empty,
    /// A `{` without its `}`, or a lone `}`.
    Unbalanced,
    BadName(String),
    UnknownKind(String),
    /// A number whose default is not an integer.
    BadDefault(String),
    /// A choice without any value.
    NoChoice(String),
    /// The same name declared twice with different types or defaults.
    Redeclared(String),
    Missing(String),
    NotNumber(String),
    NotChoice(String),
    /// A value with a NUL or a line break cannot be one word of one command.
    ControlCharacter(String),
}

impl std::fmt::Display for TemplateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => write!(f, "the command is empty"),
            Self::Unbalanced => {
                write!(f, "unbalanced braces: use {{{{ and }}}} for literal braces")
            }
            Self::BadName(name) => write!(f, "invalid parameter name {name:?}"),
            Self::UnknownKind(kind) => write!(f, "unknown parameter type {kind:?}"),
            Self::BadDefault(name) => write!(f, "the default of {name} is not a number"),
            Self::NoChoice(name) => write!(f, "{name} has no choice"),
            Self::Redeclared(name) => write!(f, "{name} is declared twice differently"),
            Self::Missing(name) => write!(f, "{name} has no value"),
            Self::NotNumber(name) => write!(f, "{name} must be a whole number"),
            Self::NotChoice(name) => write!(f, "{name} is not one of the choices"),
            Self::ControlCharacter(name) => {
                write!(f, "{name} contains a line break or a control character")
            }
        }
    }
}
impl std::error::Error for TemplateError {}

fn valid_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn is_number(value: &str) -> bool {
    let digits = value.strip_prefix('-').unwrap_or(value);
    !digits.is_empty() && digits.len() <= 18 && digits.bytes().all(|b| b.is_ascii_digit())
}

/// `name`, `name=default`, `name:number`, `name:number=10`, `name:choice=a|b`.
fn declaration(inner: &str) -> std::result::Result<(Parameter, bool), TemplateError> {
    let (head, default) = match inner.split_once('=') {
        Some((head, default)) => (head, Some(default)),
        None => (inner, None),
    };
    let (name, kind) = match head.split_once(':') {
        Some((name, kind)) => (name.trim(), Some(kind.trim())),
        None => (head.trim(), None),
    };
    if !valid_name(name) {
        return Err(TemplateError::BadName(name.to_owned()));
    }
    // A bare `{name}` reuses an earlier declaration of the same name.
    let bare = kind.is_none() && default.is_none();
    let parameter = match kind {
        None | Some("text") => Parameter {
            name: name.to_owned(),
            kind: Kind::Text,
            default: default.unwrap_or_default().to_owned(),
        },
        Some("number") => {
            let default = default.unwrap_or_default().trim();
            if !default.is_empty() && !is_number(default) {
                return Err(TemplateError::BadDefault(name.to_owned()));
            }
            Parameter {
                name: name.to_owned(),
                kind: Kind::Number,
                default: default.to_owned(),
            }
        }
        Some("choice") => {
            let choices: Vec<String> = default
                .unwrap_or_default()
                .split('|')
                .map(|choice| choice.trim().to_owned())
                .filter(|choice| !choice.is_empty())
                .collect();
            let Some(first) = choices.first().cloned() else {
                return Err(TemplateError::NoChoice(name.to_owned()));
            };
            Parameter {
                name: name.to_owned(),
                kind: Kind::Choice(choices),
                default: first,
            }
        }
        Some(other) => return Err(TemplateError::UnknownKind(other.to_owned())),
    };
    Ok((parameter, bare))
}

fn parse(template: &str) -> std::result::Result<(Vec<Part>, Vec<Parameter>), TemplateError> {
    if template.trim().is_empty() {
        return Err(TemplateError::Empty);
    }
    let mut parts = Vec::new();
    let mut parameters: Vec<Parameter> = Vec::new();
    let mut text = String::new();
    let mut chars = template.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '{' if chars.peek() == Some(&'{') => {
                chars.next();
                text.push('{');
            }
            '}' if chars.peek() == Some(&'}') => {
                chars.next();
                text.push('}');
            }
            '}' => return Err(TemplateError::Unbalanced),
            '{' => {
                let mut inner = String::new();
                loop {
                    match chars.next() {
                        Some('}') => break,
                        Some('{') | None => return Err(TemplateError::Unbalanced),
                        Some(c) => inner.push(c),
                    }
                }
                let (parameter, bare) = declaration(&inner)?;
                match parameters.iter().find(|p| p.name == parameter.name) {
                    Some(_) if bare => {}
                    Some(known) if *known == parameter => {}
                    Some(_) => return Err(TemplateError::Redeclared(parameter.name)),
                    None => parameters.push(parameter.clone()),
                }
                if !text.is_empty() {
                    parts.push(Part::Text(std::mem::take(&mut text)));
                }
                parts.push(Part::Value(parameter.name));
            }
            c => text.push(c),
        }
    }
    if !text.is_empty() {
        parts.push(Part::Text(text));
    }
    Ok((parts, parameters))
}

/// Parameters asked at each use, in order of first appearance.
pub fn parameters(template: &str) -> std::result::Result<Vec<Parameter>, TemplateError> {
    parse(template).map(|(_, parameters)| parameters)
}

/// One POSIX shell word, whatever the value holds.
pub fn quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn checked(parameter: &Parameter, value: &str) -> std::result::Result<String, TemplateError> {
    let name = || parameter.name.clone();
    if value.chars().any(char::is_control) {
        return Err(TemplateError::ControlCharacter(name()));
    }
    match &parameter.kind {
        Kind::Number => {
            let value = value.trim();
            if value.is_empty() {
                Err(TemplateError::Missing(name()))
            } else if is_number(value) {
                // Digits and an optional sign need no quoting.
                Ok(value.to_owned())
            } else {
                Err(TemplateError::NotNumber(name()))
            }
        }
        Kind::Choice(choices) => {
            if choices.iter().any(|choice| choice == value) {
                Ok(quote(value))
            } else {
                Err(TemplateError::NotChoice(name()))
            }
        }
        Kind::Text => {
            if value.is_empty() {
                Err(TemplateError::Missing(name()))
            } else {
                Ok(quote(value))
            }
        }
    }
}

/// Check one value the way `render` will, without building the command.
pub fn check(parameter: &Parameter, value: &str) -> std::result::Result<(), TemplateError> {
    checked(parameter, value).map(|_| ())
}

/// The command with each placeholder replaced by its checked, quoted value.
/// A value is always inserted as one shell word: it cannot add a command, an
/// option list or a redirection, whatever was typed.
pub fn render(
    template: &str,
    values: &BTreeMap<String, String>,
) -> std::result::Result<String, TemplateError> {
    let (parts, parameters) = parse(template)?;
    let mut command = String::new();
    for part in parts {
        match part {
            Part::Text(text) => command.push_str(&text),
            Part::Value(name) => {
                let parameter = parameters
                    .iter()
                    .find(|p| p.name == name)
                    .ok_or_else(|| TemplateError::Missing(name.clone()))?;
                let value = values
                    .get(&name)
                    .ok_or_else(|| TemplateError::Missing(name.clone()))?;
                command.push_str(&checked(parameter, value)?);
            }
        }
    }
    Ok(command)
}

/// Longest accepted name, in characters.
pub const MAX_NAME: usize = 40;

pub fn validate_name(name: &str) -> Result<()> {
    anyhow::ensure!(!name.trim().is_empty(), "command name is required");
    anyhow::ensure!(
        name == name.trim(),
        "command name cannot start or end with whitespace"
    );
    anyhow::ensure!(
        !name.chars().any(char::is_control),
        "command name cannot contain control characters"
    );
    anyhow::ensure!(
        name.chars().count() <= MAX_NAME,
        "command name is limited to {MAX_NAME} characters"
    );
    Ok(())
}

fn validate(command: &SavedCommand) -> Result<()> {
    anyhow::ensure!(
        !command.description.chars().any(char::is_control),
        "command description cannot contain control characters"
    );
    anyhow::ensure!(
        !command.template.chars().any(char::is_control),
        "command template must be a single line"
    );
    parse(&command.template)?;
    Ok(())
}

/// Personal commands are not tied to one SSH configuration: one catalogue.
#[derive(Debug)]
pub struct Commands {
    root: PathBuf,
    commands: BTreeMap<String, SavedCommand>,
}

impl Commands {
    pub fn load(root: &Path) -> Result<Self> {
        ensure_root(root)?;
        let commands = Self::entries(&Self::read(root)?)?;
        Ok(Self {
            root: root.into(),
            commands,
        })
    }

    pub fn all(&self) -> &BTreeMap<String, SavedCommand> {
        &self.commands
    }

    fn read(root: &Path) -> Result<Value> {
        let value =
            read_json(root, "commands.json")?.unwrap_or_else(|| serde_json::json!({"commands":{}}));
        anyhow::ensure!(
            value.is_object(),
            "command catalogue root must be an object"
        );
        Ok(value)
    }

    fn entries(value: &Value) -> Result<BTreeMap<String, SavedCommand>> {
        let mut result = BTreeMap::new();
        if let Some(commands) = value.get("commands") {
            for (name, entry) in commands.as_object().context("commands must be an object")? {
                validate_name(name)?;
                let command: SavedCommand = serde_json::from_value(entry.clone())
                    .with_context(|| format!("parse command {name:?}"))?;
                validate(&command).with_context(|| format!("command {name:?}"))?;
                result.insert(name.clone(), command);
            }
        }
        Ok(result)
    }

    fn update(&mut self, edit: impl FnOnce(&mut Map<String, Value>) -> Result<()>) -> Result<()> {
        let mut output = Self::read(&self.root)?;
        let commands = output
            .as_object_mut()
            .context("command catalogue root must be an object")?
            .entry("commands")
            .or_insert_with(|| serde_json::json!({}))
            .as_object_mut()
            .context("commands must be an object")?;
        edit(commands)?;
        let committed = Self::entries(&output)?;
        atomic_json(&self.root, "commands.json", &output, 0o600)?;
        self.commands = committed;
        Ok(())
    }

    /// `original` is `None` for a creation, `Some` for an edit or a rename.
    /// Another command is never overwritten.
    pub fn save(
        &mut self,
        original: Option<&str>,
        name: &str,
        command: &SavedCommand,
    ) -> Result<()> {
        validate_name(name)?;
        validate(command)?;
        let value = serde_json::to_value(command)?;
        self.update(|commands| {
            if original != Some(name) {
                anyhow::ensure!(
                    !commands.contains_key(name),
                    "a command named {name:?} already exists"
                );
            }
            if let Some(original) = original {
                anyhow::ensure!(
                    commands.remove(original).is_some(),
                    "command {original:?} no longer exists"
                );
            }
            commands.insert(name.to_owned(), value);
            Ok(())
        })
    }

    pub fn delete(&mut self, name: &str) -> Result<()> {
        self.update(|commands| {
            anyhow::ensure!(
                commands.remove(name).is_some(),
                "command {name:?} no longer exists"
            );
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn values(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
            .collect()
    }

    #[test]
    fn placeholders_declare_typed_parameters_in_order() {
        let found = parameters(
            "tail -n {lines:number=100} {file=/var/log/syslog} | grep {pattern} # {level:choice=info|warn|error}",
        )
        .unwrap();
        assert_eq!(
            found,
            [
                Parameter {
                    name: "lines".into(),
                    kind: Kind::Number,
                    default: "100".into()
                },
                Parameter {
                    name: "file".into(),
                    kind: Kind::Text,
                    default: "/var/log/syslog".into()
                },
                Parameter {
                    name: "pattern".into(),
                    kind: Kind::Text,
                    default: String::new()
                },
                Parameter {
                    name: "level".into(),
                    kind: Kind::Choice(vec!["info".into(), "warn".into(), "error".into()]),
                    default: "info".into()
                },
            ]
        );
        // A command without placeholder asks nothing; a reused name is asked once.
        assert!(parameters("uptime").unwrap().is_empty());
        assert_eq!(
            parameters("cp {file=/etc/hosts} {file}.bak").unwrap().len(),
            1
        );
    }

    #[test]
    fn values_are_inserted_as_one_quoted_word_and_never_as_shell_syntax() {
        let template = "grep {pattern} {file=/var/log/syslog} | head -n {lines:number=5}";
        assert_eq!(
            render(
                template,
                &values(&[
                    ("pattern", "error"),
                    ("file", "/var/log/app log"),
                    ("lines", "20")
                ])
            )
            .unwrap(),
            "grep 'error' '/var/log/app log' | head -n 20"
        );
        // Shell syntax typed in a value stays inside its word.
        assert_eq!(
            render(
                "echo {text}",
                &values(&[("text", "a'; rm -rf / #$(id)`id` && x | y > z")])
            )
            .unwrap(),
            "echo 'a'\\''; rm -rf / #$(id)`id` && x | y > z'"
        );
        // Literal braces are written doubled and are not parameters.
        assert_eq!(
            render("awk '{{print $1}}' {file}", &values(&[("file", "f")])).unwrap(),
            "awk '{print $1}' 'f'"
        );
        assert_eq!(
            render("cp {f} {f}.bak", &values(&[("f", "a b")])).unwrap(),
            "cp 'a b' 'a b'.bak"
        );
    }

    #[test]
    fn typed_values_are_checked_before_any_command_is_built() {
        let template = "tail -n {lines:number} {level:choice=info|warn} {name}";
        let ok = [("lines", "10"), ("level", "warn"), ("name", "x")];
        assert_eq!(
            render(template, &values(&ok)).unwrap(),
            "tail -n 10 'warn' 'x'"
        );
        for (name, value, expected) in [
            (
                "lines",
                "10; reboot",
                TemplateError::NotNumber("lines".into()),
            ),
            ("lines", "", TemplateError::Missing("lines".into())),
            ("lines", "1.5", TemplateError::NotNumber("lines".into())),
            ("level", "debug", TemplateError::NotChoice("level".into())),
            ("name", "", TemplateError::Missing("name".into())),
            (
                "name",
                "a\nb",
                TemplateError::ControlCharacter("name".into()),
            ),
        ] {
            let mut input = values(&ok);
            input.insert(name.into(), value.into());
            assert_eq!(render(template, &input), Err(expected), "{name}={value:?}");
        }
        assert_eq!(
            render(template, &values(&[("lines", "1")])),
            Err(TemplateError::Missing("level".into()))
        );
        assert_eq!(
            render("ls {n:number}", &values(&[("n", "-3")])).unwrap(),
            "ls -3"
        );
    }

    #[test]
    fn malformed_templates_are_refused_with_their_reason() {
        for (template, expected) in [
            ("", TemplateError::Empty),
            ("   ", TemplateError::Empty),
            ("echo {name", TemplateError::Unbalanced),
            ("echo name}", TemplateError::Unbalanced),
            ("echo {a{b}}", TemplateError::Unbalanced),
            ("echo {}", TemplateError::BadName(String::new())),
            ("echo {1x}", TemplateError::BadName("1x".into())),
            ("echo {a b}", TemplateError::BadName("a b".into())),
            (
                "echo {x:secret}",
                TemplateError::UnknownKind("secret".into()),
            ),
            ("echo {n:number=abc}", TemplateError::BadDefault("n".into())),
            ("echo {c:choice}", TemplateError::NoChoice("c".into())),
            ("echo {c:choice= | }", TemplateError::NoChoice("c".into())),
            ("echo {a=1} {a=2}", TemplateError::Redeclared("a".into())),
            ("echo {a} {a:number}", TemplateError::Redeclared("a".into())),
        ] {
            assert_eq!(parameters(template), Err(expected), "{template:?}");
        }
    }

    #[test]
    fn catalogue_is_saved_reloaded_and_never_overwrites_another_command() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("app");
        let mut commands = Commands::load(&root).unwrap();
        assert!(commands.all().is_empty());
        let disk = SavedCommand {
            description: "Free space".into(),
            scope: Scope::Any,
            template: "df -h {path=/}".into(),
        };
        commands.save(None, "Disk", &disk).unwrap();
        assert_eq!(Commands::load(&root).unwrap().all()["Disk"], disk);
        assert!(commands.save(None, "Disk", &disk).is_err());
        // A malformed template or name is refused before anything is written.
        let mut broken = disk.clone();
        broken.template = "df {path".into();
        assert!(commands.save(None, "Broken", &broken).is_err());
        assert!(commands.save(None, " x", &disk).is_err());
        assert_eq!(commands.all().len(), 1);
        commands.save(Some("Disk"), "Disk usage", &disk).unwrap();
        assert!(!commands.all().contains_key("Disk"));
        commands.delete("Disk usage").unwrap();
        assert!(commands.delete("Disk usage").is_err());
        // A catalogue edited by hand with a broken template is an error, not a silent skip.
        std::fs::write(
            root.join("commands.json"),
            br#"{"commands":{"x":{"template":"echo {"}}}"#,
        )
        .unwrap();
        assert!(Commands::load(&root).is_err());
    }
}
