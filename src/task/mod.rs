use serde::{Deserialize, Serialize};
use ssh2::Session;
use tera::{Context, Tera};

use std::collections::HashMap;
use std::error::Error;
use std::fmt::Display;
use std::fs;
use std::io::prelude::*;
use std::net::TcpStream;
use std::path::{Path, PathBuf};

use crate::playbook::{GlobalConfig, Host};

#[derive(Debug)]
pub enum TaskResult {
    Changed(Host, TaskKind),
    Unchanged(Host, TaskKind),
    _Failed(Host, TaskKind),
}

impl TaskResult {
    pub fn register_value(&self) -> String {
        match self {
            TaskResult::Changed(_, _) => "changed",
            TaskResult::Unchanged(_, _) => "unchanged",
            TaskResult::_Failed(_, _) => "failed",
        }
        .to_string()
    }
}

impl Display for TaskResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TaskResult::Changed(host, kind) => write!(f, "{kind}: {host} - CHANGED"),
            TaskResult::Unchanged(host, kind) => write!(f, "{kind}: {host} - UNCHANGED"),
            TaskResult::_Failed(host, kind) => write!(f, "{kind}: {host} - FAILED"),
        }
    }
}

#[derive(Debug, Deserialize, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct Task {
    #[serde(flatten)]
    kind: TaskKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    tags: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    register: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    when: Option<String>,
}

impl Display for Task {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.kind)
    }
}

impl Task {
    pub fn when(&self, _vars: &Context) -> bool {
        true
    }

    pub fn tags(&self) -> Option<&Vec<String>> {
        self.tags.as_ref()
    }

    pub fn kind(&mut self) -> &mut TaskKind {
        &mut self.kind
    }

    pub fn register(&self) -> Option<&String> {
        self.register.as_ref()
    }
}

#[derive(Debug, Deserialize, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskKind {
    Shell {
        name: String,
        command: String,

        #[serde(skip_serializing, skip_deserializing)]
        result: String,
    },
    Copy {
        name: String,
        src: String,
        dest: String,
        remote_src: Option<bool>,

        #[serde(skip_serializing, skip_deserializing)]
        result: String,
    },
    Template {
        name: String,
        src: String,
        dest: String,
        variables: HashMap<String, String>,

        #[serde(skip_serializing, skip_deserializing)]
        result: String,
    },
    SearchReplace {
        name: String,
        path: String,
        search: String,
        replace: String,

        #[serde(skip_serializing, skip_deserializing)]
        result: String,
    },
}

impl Display for TaskKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            TaskKind::Shell { name, .. }
            | TaskKind::Copy { name, .. }
            | TaskKind::Template { name, .. }
            | TaskKind::SearchReplace { name, .. } => name,
        };

        write!(f, "{name}")
    }
}

impl TaskKind {
    pub async fn execute_on_host(
        &mut self,
        host: &Host,
        context: &Context,
        global_config: &GlobalConfig,
        _local_config: Option<&GlobalConfig>,
    ) -> Result<TaskResult, Box<dyn Error>> {
        println!("{self}: {host} - START");
        let user = host.user.as_ref().unwrap_or(&global_config.user);
        let key = host.key.as_ref().unwrap_or(&global_config.key);
        let tcp = TcpStream::connect(format!("{}:22", host.address)).unwrap();
        let mut session = Session::new().unwrap();
        session.set_tcp_stream(tcp);
        session.handshake().unwrap();
        session.userauth_agent(user)?;

        if !session.authenticated() {
            session.userauth_pubkey_file(user, None, Path::new(&key), None)?;
        }

        let result = match self {
            Self::Shell {
                command,
                ref mut result,
                ..
            } => {
                let mut channel = session.channel_session()?;
                channel.exec(command)?;
                channel.read_to_string(result)?;

                TaskResult::Changed(host.clone(), self.clone())
            }
            Self::Copy {
                src,
                dest,
                remote_src,
                ..
            } => {
                let sftp = session.sftp()?;
                let src = PathBuf::from(src.clone());
                let dest = PathBuf::from(dest.clone());

                if let Some(true) = remote_src {
                    let mut remote_file = sftp.open(&src)?;
                    let mut contents = Vec::new();
                    remote_file.read_to_end(&mut contents)?;
                    let mut remote_dest = sftp.create(&dest)?;
                    remote_dest.write_all(&contents)?;
                } else {
                    let contents = fs::read(src)?;
                    let mut remote_file = sftp.create(&dest)?;
                    remote_file.write_all(&contents)?;
                }

                TaskResult::Changed(host.clone(), self.clone())
            }

            Self::Template {
                src,
                dest,
                variables,
                ..
            } => {
                let dest = PathBuf::from(dest.clone());
                let template = read_file(src)?;

                let mut context = context.clone();
                for (key, val) in variables.iter() {
                    context.insert(key, val);
                }

                let rendered_template = render_template(&template, &context)?;
                let mut remote_file = session.sftp()?.create(&dest)?;
                remote_file.write_all(rendered_template.as_bytes())?;

                TaskResult::Changed(host.clone(), self.clone())
            }

            Self::SearchReplace {
                path,
                search,
                replace,
                ..
            } => {
                let path = PathBuf::from(path.clone());
                let sftp = session.sftp()?;
                let mut remote_file = sftp.open(&path)?;
                let mut contents = String::new();
                remote_file.read_to_string(&mut contents)?;

                let re = regex::Regex::new(search.as_str())?;
                let new_contents = re.replace_all(&contents, replace.clone());

                let mut remote_file = sftp.create(&path)?;
                remote_file.write_all(new_contents.as_bytes())?;

                if contents == new_contents {
                    TaskResult::Unchanged(host.clone(), self.clone())
                } else {
                    TaskResult::Changed(host.clone(), self.clone())
                }
            }
        };

        println!("{result}");
        Ok(result)
    }
}

fn read_file<P: AsRef<Path>>(path: P) -> std::io::Result<String> {
    let contents = fs::read_to_string(path)?;
    Ok(contents)
}

fn render_template(template: &str, context: &Context) -> Result<String, Box<dyn Error>> {
    let mut tera = Tera::default();
    tera.add_raw_template("template", template)?;

    let rendered_template = tera.render("template", context)?;
    Ok(rendered_template)
}
