use crates::Crate;
use errors::*;
use regex::Regex;
use serde_regex;
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::Read;
use utils::size::Size;

static CONFIG_FILE: &'static str = "config.toml";

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct CrateConfig {
    #[serde(default = "default_false")]
    pub skip: bool,
    #[serde(default = "default_false")]
    pub skip_tests: bool,
    #[serde(default = "default_false")]
    pub quiet: bool,
    #[serde(default = "default_false")]
    pub update_lockfile: bool,
    #[serde(default = "default_false")]
    pub broken: bool,
}

fn default_false() -> bool {
    false
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct ServerConfig {
    pub bot_acl: Vec<String>,
    pub labels: ServerLabels,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct ServerLabels {
    #[serde(with = "serde_regex")]
    pub remove: Regex,
    pub experiment_queued: String,
    pub experiment_completed: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct DemoCrates {
    pub crates: Vec<String>,
    pub github_repos: Vec<String>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct SandboxConfig {
    pub memory_limit: Size,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Config {
    pub demo_crates: DemoCrates,
    pub crates: HashMap<String, CrateConfig>,
    pub github_repos: HashMap<String, CrateConfig>,
    pub server: ServerConfig,
    pub sandbox: SandboxConfig,
}

impl Config {
    pub fn load() -> Result<Self> {
        let buffer = Self::load_as_string(CONFIG_FILE)?;

        Ok(::toml::from_str(&buffer)?)
    }

    fn load_as_string(filename: &str) -> Result<String> {
        let mut buffer = String::new();
        File::open(filename)?.read_to_string(&mut buffer)?;

        Ok(buffer)
    }

    fn crate_config(&self, c: &Crate) -> Option<&CrateConfig> {
        match *c {
            Crate::Registry(ref details) => self.crates.get(&details.name),
            Crate::GitHub(ref repo) => self.github_repos.get(&repo.slug()),
            Crate::Local(_) => None,
        }
    }

    pub fn should_skip(&self, c: &Crate) -> bool {
        self.crate_config(c).map(|c| c.skip).unwrap_or(false)
    }

    pub fn should_skip_tests(&self, c: &Crate) -> bool {
        self.crate_config(c).map(|c| c.skip_tests).unwrap_or(false)
    }

    pub fn is_quiet(&self, c: &Crate) -> bool {
        self.crate_config(c).map(|c| c.quiet).unwrap_or(false)
    }

    pub fn should_update_lockfile(&self, c: &Crate) -> bool {
        self.crate_config(c)
            .map(|c| c.update_lockfile)
            .unwrap_or(false)
    }

    pub fn is_broken(&self, c: &Crate) -> bool {
        self.crate_config(c).map(|c| c.broken).unwrap_or(false)
    }

    pub fn demo_crates(&self) -> &DemoCrates {
        &self.demo_crates
    }

    pub fn check(file: &Option<String>) -> Result<()> {
        if let Some(file) = file {
            Self::check_all(&file)
        } else {
            Self::check_all(CONFIG_FILE)
        }
    }

    fn check_all(filename: &str) -> Result<()> {
        use experiments::CrateSelect;

        let buffer = Self::load_as_string(filename)?;
        let mut has_errors = Self::check_for_dup_keys(&buffer).is_err();
        let cfg: Self = ::toml::from_str(&buffer)?;
        let db = ::db::Database::open()?;
        let crates = ::crates::lists::get_crates(CrateSelect::Full, &db, &cfg)?;
        has_errors |= cfg.check_for_missing_crates(&crates).is_err();
        has_errors |= cfg.check_for_missing_repos(&crates).is_err();
        if has_errors {
            bail!("the config file contains errors");
        } else {
            Ok(())
        }
    }

    fn check_for_dup_keys(buffer: &str) -> Result<()> {
        if let Err(e) = ::toml::from_str::<::toml::Value>(&buffer) {
            error!("got error parsing the config-file: {}", e);
            Err(e.into())
        } else {
            Ok(())
        }
    }

    fn check_for_missing_crates(&self, crates: &[Crate]) -> Result<()> {
        if self.crates.is_empty() {
            return Ok(());
        }

        let mut list_of_crates: HashSet<String> = HashSet::new();
        for krate in crates {
            let name = if let Crate::Registry(ref details) = krate {
                details.name.clone()
            } else {
                continue;
            };
            list_of_crates.insert(name);
        }

        let mut any_missing = false;
        for crate_name in self.crates.keys() {
            if !list_of_crates.contains(&*crate_name) {
                error!(
                    "check-config failed: crate `{}` is not available.",
                    crate_name
                );
                any_missing = true;
            }
        }
        if any_missing {
            Err(ErrorKind::BadConfig.into())
        } else {
            Ok(())
        }
    }

    fn check_for_missing_repos(&self, crates: &[Crate]) -> Result<()> {
        if self.github_repos.is_empty() {
            return Ok(());
        }

        let mut list_of_crates: HashSet<String> = HashSet::new();
        for krate in crates {
            let name = if let Crate::GitHub(ref details) = krate {
                format!("{}/{}", details.org, details.name)
            } else {
                continue;
            };
            list_of_crates.insert(name);
        }

        let mut any_missing = false;
        for repo_name in self.github_repos.keys() {
            if !list_of_crates.contains(&*repo_name) {
                error!(
                    "check-config failed: GitHub repo `{}` is missing",
                    repo_name
                );
                any_missing = true;
            }
        }
        if any_missing {
            Err(ErrorKind::BadConfig.into())
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
impl Default for Config {
    fn default() -> Self {
        Config {
            demo_crates: DemoCrates {
                crates: vec!["lazy_static".into()],
                github_repos: vec!["brson/hello-rs".into()],
            },
            crates: HashMap::new(),
            github_repos: HashMap::new(),
            sandbox: SandboxConfig {
                memory_limit: Size::Gigabytes(2),
            },
            server: ServerConfig {
                bot_acl: Vec::new(),
                labels: ServerLabels {
                    remove: Regex::new("^$").unwrap(),
                    experiment_queued: "".into(),
                    experiment_completed: "".into(),
                },
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Config;
    use crates::{Crate, GitHubRepo, RegistryCrate};

    #[test]
    fn test_config() {
        // A sample config file loaded from memory
        let config = concat!(
            "[server]\n",
            "bot-acl = []\n",
            "[server.labels]\n",
            "remove = \"\"\n",
            "experiment-queued = \"\"\n",
            "experiment-completed = \"\"\n",
            "[demo-crates]\n",
            "crates = []\n",
            "github-repos = []\n",
            "[sandbox]\n",
            "memory-limit = \"2G\"\n",
            "[crates]\n",
            "lazy_static = { skip = true }\n",
            "\n",
            "[github-repos]\n",
            "\"rust-lang/rust\" = { quiet = true }\n" // :(
        );

        let list: Config = ::toml::from_str(&config).unwrap();

        assert!(list.should_skip(&Crate::Registry(RegistryCrate {
            name: "lazy_static".into(),
            version: "42".into(),
        })));
        assert!(!list.should_skip(&Crate::Registry(RegistryCrate {
            name: "rand".into(),
            version: "42".into(),
        })));

        assert!(list.is_quiet(&Crate::GitHub(GitHubRepo {
            org: "rust-lang".into(),
            name: "rust".into(),
        })));
        assert!(!list.is_quiet(&Crate::GitHub(GitHubRepo {
            org: "rust-lang".into(),
            name: "cargo".into(),
        })));
    }
}
