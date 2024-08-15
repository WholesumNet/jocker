use std::{cmp::min};
use indicatif::{ProgressBar, ProgressStyle};

use futures_util::{
    TryStreamExt
};

use bollard::{
    Docker,
    models::{
        // CreateImageInfo,
        // ContainerCreateResponse,
        HostConfig,
        // ContainerWaitResponse,
        // ContainerWaitExitError,
    },
    image::{
        CreateImageOptions,
    },
    container::{
        CreateContainerOptions,
        Config, 
        StartContainerOptions,
        WaitContainerOptions,
    },
};

use crate::jerror::JError;

#[derive(Debug)]
pub struct JobExecutionResult {
    pub job_id: String,
    pub exit_status_code: i64,
    pub error_message: Option<String>,
}

// import docker image
pub async fn import_docker_image<'a>(
    docker_con: &'a Docker,
    image: String
) -> Result<String, JError> {
    println!("Request to import image `{image}`");
    let image_inspect = match docker_con.inspect_registry_image(&image, None).await {
        Err(e) => {
            return Err(JError {
                who: image.clone(),
                message: e.to_string(),
            })
        },
        Ok(inspect) => inspect,        
    };
    println!("Image: `{image}`, digest: `{}`", 
        image_inspect.descriptor.digest.unwrap_or_else(|| String::from(""))
    );
    let mut downloaded = 0u64;
    // setup progress bar
    let pb = ProgressBar::new(0);
    pb.set_style(ProgressStyle::default_bar()
        .template("{msg}\n{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {bytes}/{total_bytes} ({bytes_per_sec}, {eta})")
        .map_err(|e| JError {
            who: image.clone(),
            message: e.to_string(),
        })?
        .progress_chars("#>-"));
    pb.set_message(format!("Importing `{image}`"));

    let mut import_response_stream = docker_con
        .create_image(
            Some(CreateImageOptions{
                from_image: image.clone(),
                ..Default::default()
            }),
            None,
            None
        );
    while let Some(create_image_info) = import_response_stream.try_next()
        .await
        .map_err(|e| JError {
            who: image.clone(),
            message: e.to_string(),
        })? {
        // track progress
        let mut first_chunk = true; // image size is revealed within the first chunk 
        if let Some(progress_detail) = create_image_info.progress_detail {
            let current = progress_detail.current.unwrap_or_else(|| 0i64) as u64;
            let total = progress_detail.total.unwrap_or_else(|| 0i64) as u64;
            if first_chunk == true {
                pb.set_length(total); 
                first_chunk = false;
            }
            downloaded += current;
            let progress = min(downloaded + current, total);
            pb.set_position(progress);            
        }
    }
    pb.finish_with_message(format!("Image `{image}` is ready."));

    Ok(image)
}

pub async fn run_docker_job<'a>(
    docker_con: &'a Docker,
    job_id: String,
    image: String,
    command: Vec<String>,
    src_volume: String,
) -> Result<JobExecutionResult, JError> {
    // 1- import the docker image
    if let Err(err) = import_docker_image(docker_con, image.clone()).await {
        eprintln!("Docker image import error: `{err:?}`");
        println!("Defaulting to local Docker pool.");
    }
    // 2- create and start the container
    let volume = format!("{}:{}",
        src_volume,
        "/home/prince/residue");
    let create_container_resp = docker_con
        .create_container(
            Some(CreateContainerOptions{
                name: job_id.clone(),
                platform: Some(String::from("linux/amd64")),
            }),
            Config {
                image: Some(image.clone()),
                cmd: Some(command.clone()),
                // user: Some(String::from("prince")),   
                host_config: Some(HostConfig{
                    binds: Some(vec![volume]),
                    auto_remove: Some(true),
                    ..Default::default()
                }),
                ..Default::default()
            }
        )
        .await
        .map_err(|e| JError {
            who: job_id.clone(),
            message: e.to_string(),
        })?;
    println!("Container created successfully, id: `{}`", create_container_resp.id);
    if create_container_resp.warnings.len() > 0 {
        println!("Though warnings were raised: {:#?}", create_container_resp.warnings)
    }
    //@ record the start time here, for accounting and payments
    docker_con
        .start_container(
            create_container_resp.id.as_str(),
            None::<StartContainerOptions<String>>,
        )
        .await
        .map_err(|e| JError {
            who: job_id.clone(),
            message: e.to_string(),
        })?;
    println!("Container `{}` has been started.", job_id);
    // 3- wait for the container to finish
    let mut container_wait_response_stream = docker_con
        .wait_container(
            job_id.as_str(),
            None::<WaitContainerOptions<String>>,
        );
    let mut exec_result =  JobExecutionResult {
        job_id: job_id.clone(),
        exit_status_code: -1,
        error_message: None,
    };
    while let Some(container_wait_response) = container_wait_response_stream.try_next()
        .await
        .map_err(|e| JError {
            who: job_id.clone(),
            message: e.to_string(),
        })? {
        println!("{container_wait_response:#?}");
        exec_result.exit_status_code = container_wait_response.status_code;
        if let Some(wait_error) = container_wait_response.error {
            exec_result.error_message = wait_error.message;
        }
    }
    //@ record the end time here, for accounting and payments
    
    Ok(exec_result)
}
