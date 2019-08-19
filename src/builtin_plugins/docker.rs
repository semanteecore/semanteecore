use std::fmt::{Display};
use std::io::Write;
use std::ops::Try;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use failure::Fail;

use crate::plugin::proto::{
    request,
    response::{self, PluginResponse},
};
use crate::plugin::{PluginInterface, PluginStep};
use serde::Deserialize;

#[derive(Default)]
pub struct DockerPlugin {
    cfg: Option<Config>,
    state: Option<State>,
}

impl DockerPlugin {
    pub fn new() -> Self {
        DockerPlugin::default()
    }
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
struct Config {
    repo_url: String,
    repo_branch: String,
    images: Vec<Image>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
struct Image {
    registry: Registry,
    dockerfile: PathBuf,
    namespace: Option<String>,
    name: String,
    tag: String,
    binary_path: String,
    build_cmd: String,
    exec_cmd: String,
    cleanup: bool,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
enum Registry {
    Dockerhub,
}

struct State {
    credentials: Option<Credentials>,
    version: Option<semver::Version>,
}

struct Credentials {
    username: String,
    password: String,
}

impl PluginInterface for DockerPlugin {
    fn name(&self) -> response::Name {
        PluginResponse::from_ok("docker".into())
    }

    fn methods(&self, _req: request::Methods) -> response::Methods {
        PluginResponse::from_ok(vec![
            PluginStep::PreFlight,
            PluginStep::Prepare,
            PluginStep::Publish,
        ])
    }

    fn get_default_config(&self) -> response::Config {
        unimplemented!()
    }

    fn set_config(&mut self, _req: request::Config) -> response::Null {
        unimplemented!()
    }

    fn pre_flight(&mut self, _req: request::PreFlight) -> response::PreFlight {
        let mut response = PluginResponse::builder();

        let credentials = {
            let user = std::env::var("DOCKER_USER").ok();
            let password = std::env::var("DOCKER_PASSWORD").ok();
            user.and_then(|username| password.map(|password| Credentials { username, password }))
        };

        if credentials.is_none() {
            response.warning(
                "Docker registry credentials are undefined: won't be able to push the image",
            );
            response.warning("Please set DOCKER_USER and DOCKER_PASSWORD env vars");
        }

        log::info!("Checking that docker daemon is running...");
        if let Err(err) = docker_info() {
            response.error(err);
        }

        self.state.replace(State {
            credentials,
            version: None,
        });

        response.body(()).build()
    }

    fn prepare(&mut self, req: request::Prepare) -> response::Prepare {
        {
            let state = self.state.as_mut().ok_or(DockerPluginError::MissingState)?;
            state.version.replace(req.data.clone());
        }

        PluginResponse::from_ok(vec![])
    }

    fn publish(&mut self, _req: request::Publish) -> response::Publish {
        let cfg = self.cfg.as_ref().ok_or(DockerPluginError::MissingState)?;

        let state = self.state.as_ref().ok_or(DockerPluginError::MissingState)?;

        let credentials = state
            .credentials
            .as_ref()
            .ok_or(DockerPluginError::CredentialsUndefined)?;

        let version = state
            .version
            .as_ref()
            .ok_or(DockerPluginError::MissingVersion)?;

        let version = version.to_string();

        for image in &cfg.images {
            let registry_url = match image.registry {
                Registry::Dockerhub => None,
            };

            login(registry_url, &credentials)?;

            build_image(&cfg, image)?;

            // Tag as namespace/name/tag and namespace/name/version
            let from = format!("{}:{}", image.name, image.tag);
            tag_image(&from, &get_image_path(image, &image.tag))?;
            tag_image(&from, &get_image_path(image, &version))?;

            // Publish namespace/name/tag and namespace/name/version
            push_image(image, &image.tag)?;
            push_image(image, &version)?;
        }

        PluginResponse::from_ok(())
    }
}

fn get_image_path(image: &Image, tag: &str) -> String {
    if let Some(namespace) = image.namespace.as_ref() {
        format!("{}/{}:{}", namespace, image.name, tag)
    } else {
        format!("{}:{}", image.name, tag)
    }
}

fn docker_info() -> Result<(), failure::Error> {
    let status = Command::new("docker")
        .arg("info")
        .status()
        .map_err(|_| DockerPluginError::DockerNotFound)?;

    if !status.success() {
        Err(DockerPluginError::DockerReturnedError(status.code()))?
    }

    Ok(())
}

fn build_image(cfg: &Config, image: &Image) -> Result<(), failure::Error> {
    let mut cmd = Command::new("docker");

    cmd.arg("build").arg(".docker").arg("--no-cache");

    // Set filename of Dockerfile
    cmd.arg("-f").arg(&image.dockerfile.display().to_string());

    // Set name and tag
    cmd.arg("-t").arg(&format!("{}:{}", image.name, image.tag));

    let mut set_env_var = |k, v: &dyn Display| {
        cmd.arg("--build-arg").arg(format!("{}={}", k, v));
    };

    // Set env vars
    set_env_var("REPO_URL", &cfg.repo_url);
    set_env_var("REPO_BRANCH", &cfg.repo_branch);
    set_env_var("BUILD_CMD", &image.build_cmd);
    set_env_var("BINARY_PATH", &image.binary_path);
    set_env_var("CLEANUP", &image.cleanup);
    set_env_var("EXEC_CMD", &image.exec_cmd);

    log::debug!("exec {:?}", cmd);

    let status = cmd.status()?;
    if !status.success() {
        Err(DockerPluginError::DockerReturnedError(status.code()))?
    }

    log::info!("Built image {}:{}", image.name, image.tag);

    Ok(())
}

fn tag_image(from: &str, to: &str) -> Result<(), failure::Error> {
    log::info!("tagging image {} as {}", from, to);

    let mut cmd = Command::new("docker");

    let status = cmd.arg("tag").arg(from).arg(to).status()?;

    if !status.success() {
        Err(DockerPluginError::DockerReturnedError(status.code()))?
    }

    Ok(())
}

fn login(registry_url: Option<&str>, credentials: &Credentials) -> Result<(), failure::Error> {
    log::info!("logging in as {}", credentials.username);

    let mut cmd = Command::new("docker");

    cmd.arg("login")
        .arg("--username")
        .arg(&credentials.username)
        .arg("--password-stdin");

    if let Some(url) = registry_url {
        cmd.arg(url);
    }

    let mut child = cmd.stdin(Stdio::piped()).spawn()?;

    {
        let stdin = child.stdin.as_mut().ok_or(DockerPluginError::StdioError)?;
        stdin.write_all(credentials.password.as_bytes())?;
    }

    let status = child.wait()?;

    if !status.success() {
        Err(DockerPluginError::DockerReturnedError(status.code()))?
    }

    Ok(())
}

fn push_image(image: &Image, tag: &str) -> Result<(), failure::Error> {
    let mut cmd = Command::new("docker");

    cmd.arg("push");

    let path = get_image_path(image, tag);
    log::info!("Publishing image {}", path);
    cmd.arg(path);

    let status = cmd.status()?;

    if !status.success() {
        Err(DockerPluginError::DockerReturnedError(status.code()))?
    }

    Ok(())
}

#[derive(Fail, Debug)]
enum DockerPluginError {
    #[fail(display = "DOCKER_USER or DOCKER_PASSWORD are not set, cannot push the image.")]
    CredentialsUndefined,
    #[fail(display = "state is missing: forgot to call pre_flight?")]
    MissingState,
    #[fail(display = "version is missing: forgot to call prepare?")]
    MissingVersion,
    #[fail(display = "docker command exited with error {:?}", _0)]
    DockerReturnedError(Option<i32>),
    #[fail(display = "stdio error: failed to pass password to docker process via stdin")]
    StdioError,
    #[fail(display = "'docker' not found in PATH: make sure you have the docker client installed")]
    DockerNotFound,
}
