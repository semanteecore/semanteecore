pub mod hir;

use failure::Fail;
use std::convert::TryFrom;
use std::path::{Path, PathBuf};

// TODO: Fix leakage of user config definitions into internal config representation
// BODY: This achieves three goals:
// 1. Simplify user representation and limit it to parsing-only to allow experimentation with configuration syntax
// 2. Let us break internal representation while maintaining a semi-stable user interface
// 3. Make a foundation for config syntax-level versioning
pub use self::hir::Map;
pub type Value = hir::value::Definition;
pub type ValueMap = hir::value::DefinitionMap;
pub type Plugin = hir::plugin::Definition;
pub type PluginMap = hir::plugin::DefinitionMap;
pub type Step = hir::step::Definition;
pub type StepMap = hir::step::DefinitionMap;

use plugin_api::PluginStepKind;

#[derive(Debug, Clone)]
pub enum Config {
    Monoproject(Monoproject),
    Workspace(Workspace),
}

#[derive(Debug, Clone)]
pub struct Monoproject {
    pub cfg: hir::value::DefinitionMap,
    pub plugins: hir::plugin::DefinitionMap,
    pub steps: hir::step::DefinitionMap,
}

#[derive(Debug, Clone)]
pub enum Workspace {
    Unresolved(UnresolvedWorkspace),
    Resolved(ResolvedWorkspace),
}

#[derive(Debug, Clone)]
pub struct UnresolvedWorkspace {
    pub known_members: Vec<PathBuf>,
    pub ignore_patterns: Vec<glob::Pattern>,
    pub plugins: hir::plugin::DefinitionMap,
    pub cfg: hir::value::DefinitionMap,
}

#[derive(Debug, Clone)]
pub struct ResolvedWorkspace {
    pub members: Vec<PathBuf>,
    pub cfg: hir::value::DefinitionMap,
}

impl Config {
    pub fn from_path<P: AsRef<Path>>(path: P, is_dry_run: bool) -> Result<Self, failure::Error> {
        let path = path.as_ref();
        let hir = hir::Config::from_path(path)?;

        let mut config = if hir.workspace.is_some() {
            Workspace::try_from(hir).map(Config::Workspace)?
        } else {
            Monoproject::try_from(hir).map(Config::Monoproject)?
        };

        let cfg_map = match &mut config {
            Config::Monoproject(monoproject) => &mut monoproject.cfg,
            Config::Workspace(workspace) => match workspace {
                Workspace::Resolved(resolved) => &mut resolved.cfg,
                Workspace::Unresolved(unresolved) => &mut unresolved.cfg,
            },
        };

        // Set a dry run flag to the passed state, if it wasn't defined explicitly in releaserc.toml
        cfg_map
            .entry("dry_run".to_owned())
            .or_insert_with(|| Value::Value(is_dry_run.into()));

        // Set a project root if it's not defined in release.toml
        let workspace_path = path.parent().ok_or_else(|| {
            failure::format_err!(
                "couldn't find workspace directory; try using an absolute path to config with --path option"
            )
        })?;

        let workspace_path_value = Value::Value(serde_json::to_value(workspace_path.to_owned())?);

        cfg_map.entry("project_root".into()).or_insert(workspace_path_value);

        Ok(config)
    }
}

impl TryFrom<hir::Config> for Workspace {
    type Error = failure::Error;

    fn try_from(cfg: hir::Config) -> Result<Self, Self::Error> {
        let workspace = cfg
            .workspace
            .expect("[workspace] not found in hir::Config, this is a bug");

        // Check correctness of workspace semantics first
        if workspace.auto == false {
            if !cfg.plugins.is_empty() {
                log::warn!("workspace auto-discovery is disabled, but `plugins` section is present in releaserc.toml");
                log::warn!("ignoring `plugins` section in releaserc.toml");
            }

            if workspace.members.is_empty() {
                return Err(
                    Error::InvalidWorkspace("workspace.members must not be empty if workspace.auto is false").into(),
                );
            }
        }

        if workspace.auto == true {
            if cfg.plugins.is_empty() {
                return Err(
                    Error::InvalidWorkspace("cannot discover workspace, as there are no plugins available").into(),
                );
            }
        }

        // Convert ignore patterns to glob::Pattern
        let ignore_patterns: Vec<glob::Pattern> = workspace
            .ignore
            .iter()
            .map(|pat| glob::Pattern::new(&pat))
            .collect::<Result<_, _>>()?;

        // Members list acts as a whitelist for ignore
        let known_members = workspace.members;
        let plugins = cfg.plugins;
        let cfg = cfg.cfg;

        let workspace = if workspace.auto {
            Workspace::Unresolved(UnresolvedWorkspace {
                known_members: vec![],
                ignore_patterns,
                plugins,
                cfg,
            })
        } else {
            Workspace::Resolved(ResolvedWorkspace {
                members: known_members,
                cfg,
            })
        };

        Ok(workspace)
    }
}

impl TryFrom<hir::Config> for Monoproject {
    type Error = failure::Error;

    fn try_from(cfg: hir::Config) -> Result<Self, Self::Error> {
        let steps = cfg.steps;

        for (step, def) in steps.iter() {
            match def {
                // If step is defined as singleton in the config,
                // as that's the most permissive kind,
                // we can use it for both singleton and shared steps
                Step::Singleton(_) => (),
                Step::Shared(_) | Step::Discover => match step.kind() {
                    PluginStepKind::Shared => (),
                    PluginStepKind::Singleton => {
                        return Err(Error::WrongStepKind {
                            expected: PluginStepKind::Singleton,
                            got: PluginStepKind::Shared,
                        }
                        .into())
                    }
                },
            }
        }

        Ok(Monoproject {
            cfg: cfg.cfg,
            plugins: cfg.plugins,
            steps,
        })
    }
}

#[derive(Fail, Debug, PartialEq, Eq)]
pub enum Error {
    #[fail(display = "step defined as {:?}, expected {:?}", got, expected)]
    WrongStepKind {
        expected: PluginStepKind,
        got: PluginStepKind,
    },
    #[fail(display = "invalid workspace: {}", _0)]
    InvalidWorkspace(&'static str),
}
