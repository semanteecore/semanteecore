//use std::sync::Arc;
//
//use crate::plugin_support::{PluginInterface, Plugin};
//use crate::plugin_runtime::kernel::InjectionTarget;
//use core::mem;
//use std::path::{PathBuf, Path};
//use std::fs;
//use walkdir::{WalkDir, DirEntry};
//use std::collections::HashSet;
//use std::fs::FileType;
//use crate::plugin_runtime::Kernel;
//use crate::config::Config;
//use crate::logger;
//use crate::plugin_support::keys::PROJECTS_PATHS;
//use crate::plugin_support::flow::{Availability, ProvisionCapability};
//use crate::plugin_support::proto::ProjectsPaths;
//
//pub struct Dispatcher {
//
//}
//
//pub struct DispatcherBuilder {
//    path: PathBuf,
//    injections: Vec<(Plugin, InjectionTarget)>,
//}
//
//impl DispatcherBuilder {
//    pub fn new(path: impl AsRef<Path>) -> Self {
//        DispatcherBuilder {
//            path: path.as_ref().to_owned(),
//            injections: Vec::new(),
//        }
//    }
//
//    pub fn inject_plugin(&mut self, plugin: Plugin, target: InjectionTarget) -> &mut Self {
//        self.injections.push((plugin, target));
//        self
//    }
//
//    pub fn build(self) -> Result<Dispatcher, failure::Error> {
//        let kernel = {
//            let mut builder = Kernel::builder(config.clone());
//            for (plugin, target) in injections {
//                builder.inject_plugin(plugin.clone(), *target);
//            }
//            builder.build()?
//        };
//
//        // Collect a list of plugins capable of provisioning the project structure
//        let capable_plugins = {
//            let mut filtered = Vec::new();
//            let plugins = init_kernel.get_plugins();
//            for plugin in plugins {
//                let interface = plugin.as_interface();
//
//                // Get keys that plugin can provision
//                let caps = {
//                    let _span = logger::span(&plugin.name);
//                    interface.provision_capabilities()?
//                };
//
//                // Iterate through capabilities to find the PROJECTS_PATHS key
//                let mut can_provision_project_structure = false;
//                for cap in caps {
//                    if cap.key == PROJECTS_PATHS {
//                        // Key must be available always
//                        if cap.when == Availability::Always {
//                            can_provision_project_structure = true;
//                        } else {
//                            log::warn!("invalid configuration of plugin {}", plugin.name);
//                            log::warn!("key {:?} must have {:?}", PROJECTS_PATHS, Availability::Always);
//                        }
//                    }
//                }
//
//                if can_provision_project_structure {
//                    filtered.push(plugin)
//                }
//            }
//            filtered
//        };
//
//        let roots = find_releaserc_roots(&self.path)?;
//
//        unimplemented!()
//    }
//}
//
//struct RootHandler {
//    path: PathBuf,
//    subprojects: Vec<SubProject>,
//}
//
//impl RootHandler {
//    fn try_new(path: PathBuf, is_dry_run: bool, plugins: &[&Plugin], path_filter: impl Fn(&Path) -> bool) -> Result<Self, failure::Error> {
//        let config = Config::from_toml(&path, is_dry_run)?;
//
//        let init_kernel = new_kernel()?;
//        let plugins = init_kernel.get_plugins();
//
//        let mut project_paths = Vec::new();
//        for plugin in plugins {
//            let interface = plugin.as_interface();
//
//            // Request the project structure from the plugin
//            let provided_project_paths = interface.get_value(PROJECTS_PATHS)?;
//            let mut provided_project_paths: ProjectsPaths = serde_json::from_value(provided_project_paths)?;
//
//            // Add the discovered paths
//            for path in provided_project_paths {
//                let path = PathBuf::from(path);
//                if !path.exists() {
//                    log::warn!("plugin {} returned an invalid path '{}': not found", plugin.name, path.display());
//                } else {
//                    project_paths.push(path);
//                }
//            }
//        }
//
//        let subprojects = project_paths.into_iter()
//            .filter(|path| path_filter(&path))
//            .map(|path| SubProject {
//                path,
//            })
//            .collect()?;
//
//        Ok(RootHandler {
//            path,
//            subprojects,
//        })
//    }
//}
//
//struct SubProject {
//    path: PathBuf,
//}
//
//
//fn find_releaserc_roots(path: impl AsRef<Path>) -> Result<Vec<PathBuf>, walkdir::Error> {
//    let filter_fn = |entry: DirEntry| {
//        let file_type = entry.file_type();
//        if file_type.is_dir() {
//            None
//        } else {
//            if entry.file_name() == "releaserc.toml" {
//                entry.path().parent().map(ToOwned::to_owned)
//            } else {
//                None
//            }
//        }
//    };
//
//    WalkDir::new(path)
//        .into_iter()
//        .filter_map(|entry|
//            entry
//                .map(filter_fn)
//                .transpose())
//        .collect()
//}
//
//#[cfg(test)]
//mod tests {
//    use super::*;
//    use std::fs::File;
//
//    #[test]
//    fn find_roots_simple() -> Result<(), failure::Error> {
//        let dir = tempfile::tempdir()?;
//        File::create(dir.path().join("releaserc.toml"))?;
//        let roots = find_releaserc_roots(dir.path())?;
//        assert_eq!(&roots, &[dir.path()]);
//        Ok(())
//    }
//
//    #[test]
//    fn find_roots_wrong_file_type() -> Result<(), failure::Error> {
//        let dir = tempfile::tempdir()?;
//        fs::create_dir(dir.path().join("releaserc.toml"))?;
//        let roots = find_releaserc_roots(dir.path())?;
//        assert!(roots.is_empty());
//        Ok(())
//    }
//
//    #[test]
//    fn find_roots_nested() -> Result<(), failure::Error> {
//        let dir = tempfile::tempdir()?;
//
//        let expected = &[
//            dir.path().to_owned(),
//            dir.path().join("one"),
//            dir.path().join("two"),
//        ];
//
//        for d in expected {
//            if !d.exists() {
//                fs::create_dir(d)?;
//            }
//            File::create(d.join("releaserc.toml"))?;
//        }
//
//        let roots = find_releaserc_roots(dir.path())?;
//        assert_eq!(&roots, &expected);
//
//        Ok(())
//    }
//
//    #[test]
//    fn find_roots_only_nested() -> Result<(), failure::Error> {
//        let dir = tempfile::tempdir()?;
//
//        let expected = &[
//            dir.path().join("one"),
//            dir.path().join("two"),
//        ];
//
//        for d in expected {
//            fs::create_dir(d)?;
//            File::create(d.join("releaserc.toml"))?;
//        }
//
//        let roots = find_releaserc_roots(dir.path())?;
//        assert_eq!(&roots, &expected);
//
//        Ok(())
//    }
//
//    #[test]
//    fn find_roots_symlink() -> Result<(), failure::Error> {
//        let dir = tempfile::tempdir()?;
//        let orig_file_path = dir.path().join("releaserc.toml");
//        File::create(&orig_file_path)?;
//
//        let expected = &[
//            dir.path().to_owned(),
//            dir.path().join("one"),
//            dir.path().join("two"),
//        ];
//
//        for d in expected {
//            if !d.exists() {
//                fs::create_dir(d)?;
//            }
//            let file_path = d.join("releaserc.toml");
//            if !file_path.exists() {
//                symlink::symlink_file(&orig_file_path, &file_path)?;
//            }
//        }
//
//        let roots = find_releaserc_roots(dir.path())?;
//        assert_eq!(&roots, &expected);
//
//        Ok(())
//    }
//}
//
