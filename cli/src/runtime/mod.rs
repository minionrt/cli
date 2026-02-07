use std::path::Path;

use bollard::container::{
    AttachContainerOptions, Config, LogOutput, StartContainerOptions, WaitContainerOptions,
};
use bollard::image::{BuildImageOptions, CreateImageOptions};
use bollard::models::HostConfig;
use bollard::Docker;
use futures::StreamExt;
use futures::TryStreamExt;
use uuid::Uuid;

pub struct ContainerConfig {
    pub image: String,
    pub env_vars: Vec<(String, String)>,
}

/// Runtime that uses the local Docker daemon to run containers.
pub struct LocalDockerRuntime {
    docker: Docker,
}

impl LocalDockerRuntime {
    /// Connect to the local Docker daemon.
    pub fn connect() -> anyhow::Result<Self> {
        let docker = Docker::connect_with_local_defaults()?;
        Ok(Self { docker })
    }

    /// IP address to which services on the host should bind to be accessible from containers.
    pub async fn bridge_network_ip(&self) -> anyhow::Result<String> {
        // On Windows and macOS, services bound to "localhost" are not accessible from
        // containers via "host.docker.internal".
        if running_on_windows_or_mac_os() {
            return Ok("127.0.0.1".to_string());
        }

        // On Linux, services bound to "localhost" are not accessible from containers via "host.docker.internal".
        // Instead, we bind to the IP address of the Docker bridge network gateway.
        let network = self.docker.inspect_network::<&str>("bridge", None).await?;
        let ipam = network
            .ipam
            .ok_or_else(|| anyhow::anyhow!("Missing IPAM information in network inspection"))?;
        let configs = ipam
            .config
            .ok_or_else(|| anyhow::anyhow!("Missing IPAM configuration in network inspection"))?;
        let first_config = configs
            .first()
            .ok_or_else(|| anyhow::anyhow!("IPAM configuration list is empty"))?;
        let gateway = first_config
            .gateway
            .clone()
            .ok_or_else(|| anyhow::anyhow!("Missing gateway in IPAM configuration"))?;

        Ok(gateway)
    }

    /// Build a container image from a Containerfile.
    pub async fn build_container_image<P: AsRef<Path>>(
        &self,
        containerfile: P,
    ) -> anyhow::Result<String> {
        // Generate a unique image name using a UUID.
        let image_name = format!("minion-{}", Uuid::new_v4());

        // The build context is the directory where the Containerfile is located.
        let containerfile_path = containerfile.as_ref();
        let context_dir = containerfile_path.parent().unwrap();

        // Create an in-memory tar archive of the entire build context directory.
        // This archive respects any .dockerignore rules.
        let mut archive_buffer = Vec::new();
        {
            let mut tar_builder = tar::Builder::new(&mut archive_buffer);

            // Configure WalkBuilder to use only a .dockerignore file
            // (disable default gitignore behavior).
            let walker = ignore::WalkBuilder::new(context_dir)
                .ignore(false)
                .git_ignore(false)
                .git_global(false)
                .git_exclude(false)
                .add_custom_ignore_filename(".dockerignore")
                .build();

            for result in walker {
                let entry = result?;
                let path = entry.path();
                // Get a path relative to the build context.
                let rel_path = path.strip_prefix(context_dir)?;
                // Skip the root directory itself.
                if rel_path.as_os_str().is_empty() {
                    continue;
                }
                if path.is_dir() {
                    tar_builder.append_dir(rel_path, path)?;
                } else if path.is_file() {
                    tar_builder.append_path_with_name(path, rel_path)?;
                }
            }
            tar_builder.finish()?;
        }

        // Set build options.
        // Note: The dockerfile field should match the filename of the provided containerfile.
        let build_options = BuildImageOptions {
            dockerfile: containerfile_path
                .file_name()
                .unwrap()
                .to_string_lossy()
                .into(),
            t: image_name.clone(),
            rm: true,
            ..Default::default()
        };

        // Build the image using the tar archive as the build context.
        let mut build_stream =
            self.docker
                .build_image(build_options, None, Some(archive_buffer.into()));

        while let Some(build_result) = build_stream.try_next().await? {
            if let Some(output) = build_result.stream {
                print!("{output}");
            }
        }

        Ok(image_name)
    }

    /// Pull a container image from a registry.
    pub async fn pull_container_image(&self, image: &str) -> anyhow::Result<()> {
        let options = Some(CreateImageOptions {
            from_image: image,
            ..Default::default()
        });

        let mut stream = self.docker.create_image(options, None, None);

        while let Some(result) = stream.next().await {
            result?;
        }

        Ok(())
    }

    /// Run a container with the given configuration.
    pub async fn run_container(&self, config: ContainerConfig) -> anyhow::Result<String> {
        let env: Vec<String> = config
            .env_vars
            .into_iter()
            .map(|(key, value)| format!("{key}={value}"))
            .collect();

        let host_config = HostConfig {
            extra_hosts: Some(vec!["host.docker.internal:host-gateway".to_string()]),
            ..Default::default()
        };

        let container_config = Config {
            image: Some(config.image),
            env: Some(env),
            host_config: Some(host_config),
            attach_stdout: Some(true),
            attach_stderr: Some(true),
            ..Default::default()
        };

        let container = self
            .docker
            .create_container::<&str, _>(None, container_config)
            .await?;
        self.docker
            .start_container(&container.id, None::<StartContainerOptions<String>>)
            .await?;

        let attach_options = Some(AttachContainerOptions::<&str> {
            stdout: Some(true),
            stderr: Some(true),
            stdin: None,
            stream: Some(true),
            logs: Some(true),
            ..Default::default()
        });

        let attached = self
            .docker
            .attach_container(&container.id, attach_options)
            .await?;

        let mut output_stream = attached.output;

        // Spawn a task to forward container output (stdout/stderr) to host stdout.
        let output_forwarder = tokio::spawn(async move {
            while let Some(Ok(log)) = output_stream.next().await {
                match log {
                    LogOutput::StdOut { message } => {
                        if let Ok(text) = String::from_utf8(message.to_vec()) {
                            print!("{text}");
                        }
                    }
                    LogOutput::StdErr { message } => {
                        if let Ok(text) = String::from_utf8(message.to_vec()) {
                            eprint!("{text}");
                        }
                    }
                    _ => {}
                }
            }
        });

        // Wait for the container to finish running.
        let mut wait_stream = self
            .docker
            .wait_container(&container.id, None::<WaitContainerOptions<String>>);

        if let Some(result) = wait_stream.next().await {
            let wait_msg = result?;
            if wait_msg.status_code > 0 {
                return Err(anyhow::anyhow!(
                    "Container exited with status code {}",
                    wait_msg.status_code
                ));
            }
        }

        let _ = output_forwarder.await;

        Ok(container.id)
    }

    /// Delete a container by its ID.
    pub async fn delete_container(&self, container_id: String) -> anyhow::Result<()> {
        self.docker.remove_container(&container_id, None).await?;
        Ok(())
    }
}

fn running_on_windows_or_mac_os() -> bool {
    [os_info::Type::Windows, os_info::Type::Macos].contains(&os_info::get().os_type())
}
