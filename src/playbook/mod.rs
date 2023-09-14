use async_recursion::async_recursion;
use serde::{Deserialize, Serialize};
use tera::Context;
use tokio::task;

use std::fmt::Display;
use std::fs;
use std::path::{Path, PathBuf};

use crate::task::Task;

#[derive(Debug, Deserialize, Clone, Serialize)]
pub struct Host {
    pub address: String,
    pub user: Option<String>,
    pub key: Option<String>,
}

impl Display for Host {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.address)
    }
}

#[derive(Debug, Deserialize, Clone, Serialize)]
pub struct HostConfig {
    pub global_config: GlobalConfig,
    pub hosts: Vec<Host>,
}

impl TryFrom<PathBuf> for HostConfig {
    type Error = serde_yaml::Error;

    fn try_from(value: PathBuf) -> Result<Self, Self::Error> {
        let contents = read_file(value).expect("failed to read file");
        Self::try_from(contents)
    }
}

impl TryFrom<String> for HostConfig {
    type Error = serde_yaml::Error;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        serde_yaml::from_str(&value)
    }
}

impl TryFrom<Vec<u8>> for HostConfig {
    type Error = serde_yaml::Error;

    fn try_from(value: Vec<u8>) -> Result<Self, Self::Error> {
        serde_yaml::from_str(std::str::from_utf8(&value).expect("failed to read utf8"))
    }
}

#[derive(Debug, Deserialize, Clone, Serialize)]
pub struct GlobalConfig {
    pub user: String,
    pub key: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct Include {
    #[serde(flatten)]
    file: PathBuf,
    tags: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    when: Option<String>,
}

#[derive(Debug, Deserialize, Clone, Serialize)]
pub struct Playbook {
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    include: Option<Vec<Include>>,
    hosts: Vec<String>,
    local_config: Option<GlobalConfig>,
    tasks: Vec<Task>,
}

impl Playbook {
    #[async_recursion]
    pub async fn process(&mut self, host_config: HostConfig, specified_tags: Option<Vec<String>>) {
        if let Some(included_playbooks) = &self.include {
            for include in included_playbooks {
                // eval when
                let mut included_config =
                    Playbook::try_from(include.file.clone()).expect("failed to read playbook");
                included_config
                    .process(host_config.clone(), specified_tags.clone())
                    .await;
            }
        }

        let matching_hosts = host_config
            .hosts
            .iter()
            .filter(|host| self.hosts.contains(&host.address))
            .collect::<Vec<&Host>>();

        let context = Context::new();
        // gatcher facts

        let task_handles = matching_hosts
            .into_iter()
            .map(|host| {
                let mut context = context.clone();
                let playbook = self.clone();
                let global_config = host_config.global_config.clone();
                let local_config = playbook.local_config.clone();
                let host = host.clone();
                let specified_tags = specified_tags.clone();

                task::spawn(async move {
                    for mut task in playbook.tasks {
                        if !task.when(&context) {
                            continue;
                        }

                        if let Some(specified_tags) = &specified_tags {
                            if let Some(task_tags) = &task.tags() {
                                if task_tags.iter().all(|tag| !specified_tags.contains(tag)) {
                                    continue;
                                }
                            } else {
                                continue;
                            }
                        }

                        let result = task
                            .kind()
                            .execute_on_host(&host, &context, &global_config, local_config.as_ref())
                            .await
                            .expect("failed to execute task");

                        if let Some(register_key) = task.register() {
                            context.insert(register_key.to_owned(), &result.register_value());
                        }
                    }
                })
            })
            .collect::<Vec<_>>();

        for handle in task_handles {
            handle.await.unwrap();
        }
    }
}

impl TryFrom<PathBuf> for Playbook {
    type Error = serde_yaml::Error;

    fn try_from(value: PathBuf) -> Result<Self, Self::Error> {
        let contents = read_file(value).expect("failed to read file");
        Self::try_from(contents)
    }
}

impl TryFrom<String> for Playbook {
    type Error = serde_yaml::Error;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        serde_yaml::from_str(&value)
    }
}

impl TryFrom<&String> for Playbook {
    type Error = serde_yaml::Error;

    fn try_from(value: &String) -> Result<Self, Self::Error> {
        serde_yaml::from_str(value)
    }
}

fn read_file<P: AsRef<Path>>(path: P) -> std::io::Result<String> {
    let contents = fs::read_to_string(path)?;
    Ok(contents)
}
